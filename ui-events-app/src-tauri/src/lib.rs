use anyhow::Result;
use tokio::sync::mpsc;

use tracing::{error, info};
use ui_events::create_listener;
use ui_events::server::run_server;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub async fn run() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("starting ui-events...");

    // Create a channel for communication between listener and server
    let (tx, rx) = mpsc::channel(100); // Buffer size 100

    // Create and run the platform-specific listener in a blocking thread
    let listener = create_listener()?;
    let _listener_handle = tokio::task::spawn_blocking(move || {
        if let Err(e) = listener.run(tx) {
            error!("listener error: {}", e);
        }
    });

    // Run the WebSocket server in a separate tokio task
    let _server_handle = tokio::spawn(async move {
        if let Err(e) = run_server(9001, rx).await {
            error!("server error: {}", e);
        }
    });

    // Run the Tauri application event loop
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
