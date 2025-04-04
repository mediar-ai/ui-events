use crate::event::UiEvent;
use anyhow::Result;
use tokio::sync::mpsc;
use tracing::info;

// Modules for each platform
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "windows")]
pub mod windows;

/// Common trait for platform-specific listeners.
/// Must be Send to allow spawning in a separate thread/task.
pub trait PlatformListener: Send {
    // Note: Needs Send + Sync bounds if used across threads without careful handling
    // Using blocking run for now to simplify CFRunLoop integration on macOS.
    fn run(&self, sender: mpsc::Sender<UiEvent>) -> Result<()>;
}

pub fn listener_run(tx: mpsc::Sender<UiEvent>) {
    #[cfg(target_os = "macos")]
    {
        use cidre::ns;
        macos::MacosListener::new_on_main_thread(tx).unwrap();
        ns::App::shared().run()
    }
}

/// Creates the appropriate platform listener.
pub fn create_listener() -> Result<Box<dyn PlatformListener>> {
    #[cfg(target_os = "macos")]
    {
        use macos::MacosListener;
        info!("creating macos listener");
        todo!();
        // let listener = MacosListener::new()?;
        // Ok(Box::new(listener))
    }
    #[cfg(target_os = "windows")]
    {
        use windows::WindowsListener;
        info!("creating windows listener");
        let listener = WindowsListener::new()?;
        Ok(Box::new(listener))
    }
    #[cfg(target_os = "linux")]
    {
        use linux::LinuxListener;
        info!("creating linux listener");
        let listener = LinuxListener::new()?;
        Ok(Box::new(listener))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        anyhow::bail!("unsupported platform")
    }
}
