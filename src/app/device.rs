use std::sync::{Arc, Weak};

use anyhow::anyhow;
use buttplug_client::ButtplugClientDevice;
use tokio::sync::Notify;

use crate::buttplug::deferred::DiscoveredDevice;

#[derive(Debug, Clone)]
pub struct AppDevice {
  name: String,
  address: Option<String>,
  pretty_name: Option<String>,
  approval: Option<Arc<Notify>>,
  connected_device: Option<ButtplugClientDevice>,
  server: Weak<buttplug_server::ButtplugServer>,
}

impl AppDevice {
  pub fn from_discovered(
    discovered: DiscoveredDevice,
    server: &Arc<buttplug_server::ButtplugServer>,
  ) -> Self {
    Self {
      name: discovered.name.clone(),
      address: discovered.address.clone(),
      pretty_name: None,
      approval: Some(discovered.approval()),
      connected_device: None,
      server: Arc::downgrade(server),
    }
  }

  pub fn from_connected(
    device: ButtplugClientDevice,
    server: &Arc<buttplug_server::ButtplugServer>,
  ) -> Self {
    Self {
      name: device.name().clone(),
      address: None,
      pretty_name: Some(device.name().clone()),
      approval: None,
      connected_device: Some(device),
      server: Arc::downgrade(server),
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn address(&self) -> Option<&str> {
    self.address.as_deref()
  }

  pub fn pretty_name(&self) -> Option<&str> {
    self.pretty_name.as_deref()
  }

  pub fn buttplug_index(&self) -> Option<u32> {
    self.connected_device.as_ref().map(|d| d.index())
  }

  pub fn client_device(&self) -> Option<&ButtplugClientDevice> {
    self.connected_device.as_ref()
  }

  pub fn is_connected(&self) -> bool {
    self.connected_device.is_some()
  }

  pub fn set_connected_device(&mut self, device: ButtplugClientDevice) {
    self.pretty_name = Some(device.name().clone());
    self.connected_device = Some(device);
  }

  pub fn clear_connected_device(&mut self) {
    self.connected_device = None;
  }

  pub async fn connect(&mut self) -> anyhow::Result<()> {
    if self.is_connected() {
      return Ok(());
    }

    let approval = self
      .approval
      .take()
      .ok_or_else(|| anyhow!("Device '{}' is not available to connect", self.name))?;

    approval.notify_one();
    Ok(())
  }

  pub async fn disconnect(&mut self) -> anyhow::Result<()> {
    let Some(device) = self.connected_device.clone() else {
      return Ok(());
    };

    if let Err(e) = device.stop().await {
      log::warn!("Error stopping device '{}' before disconnect: {:?}", self.name, e);
    }

    let Some(server) = self.server.upgrade() else {
      return Err(anyhow!(
        "No Buttplug server available to disconnect '{}'",
        self.name
      ));
    };

    server
      .device_manager()
      .disconnect_device(device.index())
      .await
      .map_err(|e| anyhow!("Error disconnecting '{}': {:?}", self.name, e))?;

    self.clear_connected_device();
    Ok(())
  }
}
