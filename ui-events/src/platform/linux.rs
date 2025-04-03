#![cfg(target_os = "linux")]

use super::PlatformListener;
use crate::event::UiEvent;
use anyhow::Result;
use tokio::sync::mpsc;

pub struct LinuxListener {}

impl LinuxListener {
    pub fn new() -> Result<Self> {
        anyhow::bail!("linux listener not implemented")
    }
}

impl PlatformListener for LinuxListener {
    fn run(&self, _sender: mpsc::Sender<UiEvent>) -> Result<()> {
        println!("linux listener run (unimplemented)");
        // TODO: Implement using AT-SPI
        anyhow::bail!("linux listener not implemented")
    }
}
