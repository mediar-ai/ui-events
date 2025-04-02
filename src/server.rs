// Placeholder for websocket server implementation

use crate::event::UiEvent;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc};
use tokio_tungstenite::{accept_async, tungstenite::protocol::Message};
use tracing::{debug, error, info, warn};

async fn handle_connection(
    peer: SocketAddr,
    stream: TcpStream,
    mut broadcast_rx: broadcast::Receiver<String>, // Receiver for serialized events
) -> Result<()> {
    let ws_stream = accept_async(stream)
        .await
        .context("error during websocket handshake")?;
    info!(%peer, "new websocket connection established");

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    loop {
        tokio::select! {
            // Forward broadcast messages (serialized UI events) to the client
            Ok(msg_str) = broadcast_rx.recv() => {
                if let Err(e) = ws_sender.send(Message::Text(msg_str)).await {
                    // Error likely means client disconnected
                    warn!(%peer, error = %e, "failed to send message to client, disconnecting");
                    break; // Exit loop to close connection
                }
            }
            // Handle messages *from* the client (e.g., ping/pong, close)
            Some(msg_result) = ws_receiver.next() => {
                match msg_result {
                    Ok(msg) => {
                        match msg {
                            Message::Text(_) | Message::Binary(_) => {
                                // Ignore data messages from client for now
                                debug!(%peer, "received data message (ignoring)");
                            }
                            Message::Ping(ping_data) => {
                                debug!(%peer, "received ping, sending pong");
                                if let Err(e) = ws_sender.send(Message::Pong(ping_data)).await {
                                     warn!(%peer, error = %e, "failed to send pong, disconnecting");
                                     break;
                                }
                            }
                            Message::Close(_) => {
                                info!(%peer, "received close frame, closing connection");
                                break; // Exit loop
                            }
                            Message::Pong(_) => {
                                // Usually we only send pings and expect pongs
                                debug!(%peer, "received unsolicited pong (ignoring)");
                            }
                           Message::Frame(_) => {
                                // Low-level frame, ignore in typical usage
                           }
                        }
                    }
                    Err(e) => {
                        // Tungstenite error (connection closed, protocol error, etc.)
                        warn!(%peer, error = %e, "websocket error, closing connection");
                        break; // Exit loop
                    }
                }
            }
            else => {
                // Both streams have potentially ended
                break;
            }
        }
    }

    info!(%peer, "websocket connection closed");
    // Attempt to close the sender cleanly (optional)
    let _ = ws_sender.close().await;
    Ok(())
}

pub async fn run_server(port: u16, mut rx: mpsc::Receiver<UiEvent>) -> Result<()> {
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)
        .await
        .context(format!("failed to bind websocket server to {}", addr))?;
    info!("websocket server listening on ws://{}", addr);

    // Broadcast channel for distributing serialized events to clients
    // Capacity should be chosen based on expected event volume and client processing speed
    let (broadcast_tx, _) = broadcast::channel::<String>(100); // Sender and a placeholder receiver

    // Task to receive UI events, serialize them, and broadcast
    let broadcaster_tx = broadcast_tx.clone(); // Clone sender for the task
    tokio::spawn(async move {
        info!("event broadcaster task started");
        while let Some(event) = rx.recv().await {
            match serde_json::to_string(&event) {
                Ok(json_str) => {
                    // Send to broadcast channel. If no clients are listening, the error is ignored.
                    if let Err(e) = broadcaster_tx.send(json_str) {
                        // This error typically means no clients are connected.
                        // It can be noisy, so maybe log only once or use debug level.
                        debug!("broadcast send error (no receivers?): {}", e);
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to serialize uievent to json");
                    // Decide if you want to skip the event or panic
                }
            }
        }
        info!("event broadcaster task finished (mpsc channel closed)");
        // rx is dropped here when the loop finishes (sender in main/listener dropped)
    });

    // Main loop to accept incoming connections
    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                info!(%peer, "accepting new tcp connection");
                let broadcast_rx = broadcast_tx.subscribe(); // Create a receiver for this specific client
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(peer, stream, broadcast_rx).await {
                        error!(%peer, error = %e, "error handling connection");
                    }
                });
            }
            Err(e) => {
                error!(error = %e, "failed to accept incoming tcp connection");
                // Consider if this error is recoverable or requires stopping the server
                // For now, just log and continue trying to accept
            }
        }
    }

    // Note: The loop above runs indefinitely. In a real application, you'd
    // want a mechanism for graceful shutdown (e.g., listening for a signal
    // or another channel message) to break the loop and allow tasks to finish.
    // Ok(()) // Unreachable in the current form
}
