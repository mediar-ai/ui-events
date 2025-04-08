use anyhow::Result;
use cidre::ns;
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

// #[tokio::main]
fn main() {
    tracing_subscriber::fmt::init();
    info!("starting ui-events...");

    let port = Args::parse().port;

    // Create a channel for communication between listener and server
    let (tx, rx) = mpsc::channel(100); // Buffer size 100

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();

    rt.spawn(async move {
        run_server(port, rx).await.unwrap();
        ns::App::shared().terminate(None);
    });

    platform::listener_run(tx);
}
