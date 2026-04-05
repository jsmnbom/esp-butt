use std::{
  collections::HashMap,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

use buttplug_core::ButtplugResultFuture;
use buttplug_server::device::hardware::communication::{
  HardwareCommunicationManager,
  HardwareCommunicationManagerBuilder,
  HardwareCommunicationManagerEvent,
};
use futures::FutureExt;
use log::trace;
use rustc_hash::FxBuildHasher;
use tokio::sync::mpsc::Sender;

use super::hardware::BleHardwareConnector;
use crate::ble::{
  AdEventType,
  AdReport,
  Address,
  Discovery,
  DiscoveryListener,
  PeripheralProperties,
};

#[derive(Clone)]
pub struct BleCommunicationManagerBuilder {}

impl Default for BleCommunicationManagerBuilder {
  fn default() -> Self {
    Self {}
  }
}

impl HardwareCommunicationManagerBuilder for BleCommunicationManagerBuilder {
  fn finish(
    &mut self,
    sender: Sender<HardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager> {
    Box::new(BleCommunicationManager::new(sender))
  }
}

pub struct BleCommunicationManager {
  sender: Sender<HardwareCommunicationManagerEvent>,
  scanning_status: Arc<AtomicBool>,
  peripherals: HashMap<Address, PeripheralProperties, FxBuildHasher>,
}

impl BleCommunicationManager {
  pub fn new(sender: Sender<HardwareCommunicationManagerEvent>) -> Self {
    Self {
      sender,
      scanning_status: Arc::new(AtomicBool::new(false)),
      peripherals: HashMap::with_capacity_and_hasher(64, FxBuildHasher),
    }
  }

  fn maybe_add_peripheral(
    sender: &Sender<HardwareCommunicationManagerEvent>,
    properties: &PeripheralProperties,
  ) {
    if properties.name.is_empty() || properties.services.is_empty() {
      trace!(
        "Ignoring peripheral with no name or no services: {:?}",
        properties
      );
      return;
    }
    let name = properties.name.to_string();
    let address = format!("{}", properties.address);

    let creator = Box::new(BleHardwareConnector::new(properties));
    if sender
      .try_send(HardwareCommunicationManagerEvent::DeviceFound {
        name,
        address,
        creator,
      })
      .is_err()
    {
      log::warn!(
        "Failed to send device found event for {:?}",
        properties.address
      );
    }
  }
}

impl HardwareCommunicationManager for BleCommunicationManager {
  fn name(&self) -> &'static str {
    "BleCommunicationManager"
  }

  fn start_scanning(&mut self) -> ButtplugResultFuture {
    self.scanning_status.store(true, Ordering::Relaxed);
    Discovery::new(self).start();
    async { Ok(()) }.boxed()
  }

  fn stop_scanning(&mut self) -> ButtplugResultFuture {
    self.scanning_status.store(false, Ordering::Relaxed);
    Discovery::<Self>::stop();
    async { Ok(()) }.boxed()
  }

  fn scanning_status(&self) -> bool {
    self.scanning_status.load(Ordering::Relaxed)
  }

  fn can_scan(&self) -> bool {
    true
  }
}

impl DiscoveryListener for BleCommunicationManager {
  fn on_report(&mut self, report: &AdReport) {
    if !matches!(
      report.event_type,
      AdEventType::AdvInd | AdEventType::ScanRsp
    ) {
      return;
    }
    let entry = self
      .peripherals
      .entry(report.address)
      .or_insert_with(|| PeripheralProperties::new(report.address, report.rssi));
    if let Err(e) = entry.update(report) {
      log::warn!("Failed to update peripheral properties: {:?}", e);
    }

    Self::maybe_add_peripheral(&self.sender, entry);
  }

  fn on_complete(&mut self) {
    log::info!(
      "BLE discovery complete. Found {} peripherals.",
      self.peripherals.len()
    );
    // No-op
  }
}
