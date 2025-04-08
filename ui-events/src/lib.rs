pub mod error;
pub mod event;
pub mod platform;
pub mod server;

pub use platform::create_listener;
pub use server::run_server;
use tokio::sync::mpsc;
use tracing::info;

pub fn run(port: u16) {
    let _ = tracing_subscriber::fmt::try_init();
    info!("starting ui-events...");

    // Create a channel for communication between listener and server
    let (tx, rx) = mpsc::channel(100); // Buffer size 100

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();

    use cidre::ns;

    rt.spawn(async move {
        run_server(port, rx).await.unwrap();
        ns::App::shared().terminate(None);
    });

    platform::listener_run(tx);
}
