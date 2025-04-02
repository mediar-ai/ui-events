use anyhow::Result;
use futures_util::StreamExt;
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use url::Url;

// Define the event structure again here for deserialization,
// or better, make ui-events a library crate and depend on it.
// For now, let's just print the raw JSON string.

#[tokio::main]
async fn main() -> Result<()> {
    let server_url = "ws://localhost:9001";
    println!("connecting to {}", server_url);

    let url = Url::parse(server_url)?;

    let (ws_stream, _response) = connect_async(url).await.expect("failed to connect");
    println!("websocket handshake has been successfully completed");

    let (mut _write, mut read) = ws_stream.split();

    // We just read messages in this simple client
    while let Some(msg) = read.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                println!("received: {}", text);
                // Optional: Deserialize into UiEvent struct if defined
                // let event: Result<UiEvent, _> = serde_json::from_str(&text);
                // match event {
                //     Ok(parsed_event) => println!("parsed: {:?}", parsed_event),
                //     Err(e) => eprintln!("failed to parse event: {}", e),
                // }
            }
            Ok(Message::Binary(_)) => {
                println!("received binary message (unexpected)");
            }
            Ok(Message::Ping(_)) => {
                // tokio-tungstenite handles ping/pong automatically
            }
            Ok(Message::Pong(_)) => {
                // tokio-tungstenite handles ping/pong automatically
            }
            Ok(Message::Close(close_frame)) => {
                println!("connection closed: {:?}", close_frame);
                break;
            }
            Err(e) => {
                eprintln!("websocket error: {}", e);
                break;
            }
            _ => {}
        }
    }

    Ok(())
}
