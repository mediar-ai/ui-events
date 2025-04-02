use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;

mod error;
mod event;
mod platform;
mod server;

use platform::create_listener;
use server::run_server;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// WebSocket server port
    #[clap(short, long, value_parser, default_value_t = 9001)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("starting ui-events...");

    let args = Args::parse();

    // Create a channel for communication between listener and server
    let (tx, rx) = mpsc::channel(100); // Buffer size 100

    // Create and run the platform-specific listener
    // Note: The listener's run method might be blocking (e.g., for CFRunLoop on macOS)
    // So we run it in a separate blocking task/thread.
    let listener = create_listener()?;
    let listener_handle = tokio::task::spawn_blocking(move || {
        if let Err(e) = listener.run(tx) {
            error!("listener error: {}", e); // Use proper logging
        }
    });

    // Run the WebSocket server
    let server_handle = run_server(args.port, rx);

    // Keep the application running
    // We can await both handles, though the listener might run indefinitely
    // or error out.
    tokio::select! {
        res = listener_handle => {
             match res {
                 Ok(_) => info!("listener task completed."),
                 Err(e) => error!("listener task panicked or failed: {}", e),
             }
        }
        res = server_handle => {
            match res {
                Ok(_) => info!("server task completed."),
                Err(e) => error!("server task failed: {}", e),
            }
        }
    }

    Ok(())
}
