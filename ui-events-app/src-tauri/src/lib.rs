use anyhow::Result;
use std::thread;
use tokio::sync::mpsc;
use tracing::info;
use ui_events::{platform::listener_run, run_server}; // Import necessary components // Import thread

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

    // Spawn the server task using Tauri's async runtime
    tauri::async_runtime::spawn(async move {
        if let Err(e) = run_server(9001, rx).await {
            tracing::error!("ui-events server failed: {}", e);
        }
    });

    // Spawn the listener task on a separate thread
    // This assumes listener_run does not strictly require the main thread
    let listener_tx = tx.clone();
    thread::spawn(move || {
        info!("starting ui-events listener thread...");
        listener_run(listener_tx); // This might block this thread
        info!("ui-events listener thread finished."); // May not be reached if listener_run loops indefinitely
    });

    info!("starting tauri application...");
    // Run the Tauri application event loop (this blocks the main thread)
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    Ok(())
}
