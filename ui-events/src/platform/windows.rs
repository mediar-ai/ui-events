#![cfg(target_os = "windows")]

use super::PlatformListener;
use crate::event::UiEvent;
use anyhow::Result;
use tokio::sync::mpsc;

pub struct WindowsListener {}

impl WindowsListener {
    pub fn new() -> Result<Self> {
        anyhow::bail!("windows listener not implemented")
    }
}

impl PlatformListener for WindowsListener {
    fn run(&self, _sender: mpsc::Sender<UiEvent>) -> Result<()> {
        println!("windows listener run (unimplemented)");
        // TODO: Implement using UI Automation
        anyhow::bail!("windows listener not implemented")
    }
}
