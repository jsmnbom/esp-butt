use std::sync::{Arc, Weak};

use anyhow::anyhow;
use buttplug_client::ButtplugClientDevice;
use buttplug_core::message::InputType;
use tokio::sync::{Notify, watch};

use crate::buttplug::backdoor::DiscoveredDevice;

#[derive(Debug, Clone)]
pub struct AppDevice {
  name: String,
  address: String,
  pretty_name: Option<String>,
  approval: Option<Arc<Notify>>,
  connected_device: Option<ButtplugClientDevice>,
  is_connecting: bool,
  server: Weak<buttplug_server::ButtplugServer>,
  rssi_rx: Option<watch::Receiver<i8>>,
  rssi_notify: Option<Arc<tokio::sync::Notify>>,
  rssi: Option<i8>,
  last_rssi_read: Option<std::time::Instant>,
  battery_read_fail_count: u8,
  battery: Option<u32>,
  last_battery_read: Option<std::time::Instant>,
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
      is_connecting: false,
      server: Arc::downgrade(server),
      rssi_rx: discovered.rssi_rx,
      rssi_notify: Some(discovered.rssi_notify),
      #[cfg(target_os = "espidf")]
      rssi: None,
      #[cfg(not(target_os = "espidf"))]
      rssi: Some(-30), // Mock devices are always -30, which is a strong signal. This is just to make testing easier.
      last_rssi_read: None,
      battery_read_fail_count: 0,
      battery: None,
      last_battery_read: None,
    }
  }

  pub fn name(&self) -> &str {
    &self.name
  }

  pub fn address(&self) -> &str {
    &self.address
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

  pub fn is_connecting(&self) -> bool {
    self.is_connecting
  }

  pub fn set_connected_device(&mut self, device: ButtplugClientDevice) {
    self.pretty_name = Some(device.name().clone());
    self.connected_device = Some(device);
    self.is_connecting = false;
  }

  pub fn clear_connected_device(&mut self) {
    self.connected_device = None;
    self.is_connecting = false;
  }

  pub async fn connect(&mut self) -> anyhow::Result<()> {
    if self.is_connected() {
      return Ok(());
    }

    let approval = self
      .approval
      .take()
      .ok_or_else(|| anyhow!("Device '{}' is not available to connect", self.name))?;

    self.is_connecting = true;

    approval.notify_one();
    Ok(())
  }

  pub async fn disconnect(&mut self) -> anyhow::Result<()> {
    let Some(device) = self.connected_device.clone() else {
      return Ok(());
    };

    if let Err(e) = device.stop().await {
      log::warn!(
        "Error stopping device '{}' before disconnect: {:?}",
        self.name,
        e
      );
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

    Ok(())
  }

  pub fn rssi(&self) -> Option<i8> {
    self.rssi
  }

  pub fn battery(&self) -> Option<u32> {
    self.battery
  }

  pub async fn tick(&mut self) -> anyhow::Result<bool> {
    let mut rssi_changed = false;
    if let Some(rx) = &mut self.rssi_rx {
      if rx.has_changed().unwrap_or(false) {
        let new_rssi = *rx.borrow_and_update();
        log::debug!("[{}] rssi_rx updated: {}", self.address, new_rssi);
        self.rssi = Some(new_rssi);
        rssi_changed = true;
      }
    }
    let now = std::time::Instant::now();
    if self.is_connected() {
      if self.last_rssi_read.map_or(true, |t| {
        now.duration_since(t) >= std::time::Duration::from_secs(3)
      }) {
        if let Some(notify) = &self.rssi_notify {
          notify.notify_one();
        }
        self.last_rssi_read = Some(now);
      }
    }

    let mut battery_changed = false;
    if self.is_connected()
      && self.last_battery_read.map_or(true, |t| {
        now.duration_since(t) >= std::time::Duration::from_secs(30)
      })
      && self.battery_read_fail_count < 3
    {
      if let Some(device) = self.connected_device.clone() {
        if device.input_available(InputType::Battery) {
          log::debug!("[{}] reading battery level...", self.address);
          match device.battery().await {
            Ok(level) => {
              log::debug!("[{}] battery: {}%", self.address, level);
              self.battery = Some(level);
              battery_changed = true;
              self.battery_read_fail_count = 0;
            }
            Err(e) => {
              log::warn!("[{}] battery read error: {:?}", self.address, e);
              self.battery_read_fail_count += 1;
            }
          }
        }
      }
      self.last_battery_read = Some(now);
    }

    Ok(rssi_changed || battery_changed)
  }
}
