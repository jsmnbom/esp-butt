use std::sync::Arc;

use tokio::sync::{Notify, watch};

/// Represents a device that buttplug has matched to a known protocol and is ready to
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
  pub name: String,
  pub address: String,
  pub rssi_rx: Option<watch::Receiver<i8>>,
  pub rssi_notify: std::sync::Arc<tokio::sync::Notify>,
  pub(crate) approve: Arc<Notify>,
}

impl DiscoveredDevice {
  pub fn approval(&self) -> Arc<Notify> {
    self.approve.clone()
  }
}

#[derive(Debug, Clone)]
pub enum ButtplugBackdoorEvent {
  DeviceDiscovered(DiscoveredDevice),
}
