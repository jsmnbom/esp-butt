use std::{
  collections::HashMap,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

use buttplug_core::ButtplugResultFuture;
use buttplug_server::device::hardware::communication::HardwareCommunicationManager;
use futures::FutureExt;
use log::trace;
use rustc_hash::FxBuildHasher;
use tokio::sync::mpsc::{self, Sender};

use super::hardware::BleHardwareConnector;
use crate::{
  ble::{AdEventType, AdReport, Address, Discovery, DiscoveryListener, PeripheralProperties},
  buttplug::deferred::{
    CustomHardwareCommunicationManagerBuilder,
    CustomHardwareCommunicationManagerEvent,
  },
};

#[derive(Clone, Default)]
pub struct BleCommunicationManagerBuilder {}

impl CustomHardwareCommunicationManagerBuilder for BleCommunicationManagerBuilder {
  fn finish(
    &mut self,
    sender: Sender<CustomHardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager> {
    Box::new(BleCommunicationManager::new(sender))
  }
}

pub struct BleCommunicationManager {
  sender: Sender<CustomHardwareCommunicationManagerEvent>,
  scanning_status: Arc<AtomicBool>,
  peripherals: HashMap<Address, PeripheralProperties, FxBuildHasher>,
}

impl BleCommunicationManager {
  pub fn new(sender: Sender<CustomHardwareCommunicationManagerEvent>) -> Self {
    Self {
      sender,
      scanning_status: Arc::new(AtomicBool::new(false)),
      peripherals: HashMap::with_capacity_and_hasher(64, FxBuildHasher),
    }
  }

  fn maybe_add_peripheral(
    sender: &mpsc::Sender<CustomHardwareCommunicationManagerEvent>,
    properties: &mut PeripheralProperties,
  ) {
    if properties.name.is_empty() || properties.services.is_empty() {
      trace!(
        "Ignoring peripheral with no name or no services: {:?}",
        properties
      );
      return;
    }
    let name = properties.name.to_string();
    let address = properties.address.to_string();
    let rssi_rx = properties.take_rssi_receiver();
    let rssi_notify = Arc::new(tokio::sync::Notify::new());
    let rssi_tx = properties.rssi_sender();

    let creator = Box::new(BleHardwareConnector::new(
      properties,
      rssi_notify.clone(),
      rssi_tx,
    ));
    if sender
      .try_send(CustomHardwareCommunicationManagerEvent::DeviceFound {
        name,
        address: address.clone(),
        rssi_rx,
        rssi_notify,
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
    Discovery::new(self)
      .filter_duplicates(false)
      .start();
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

    let updated = {
      let entry = self
        .peripherals
        .entry(report.address)
        .or_insert_with(|| PeripheralProperties::new(report));
      match entry.update(report) {
        Ok(updated) => updated,
        Err(e) => {
          log::warn!("Failed to update peripheral properties: {:?}", e);
          false
        }
      }
    };
    if updated {
      if let Some(entry) = self.peripherals.get_mut(&report.address) {
        Self::maybe_add_peripheral(&self.sender, entry);
      }
    }
  }

  fn on_complete(&mut self) {
    log::info!(
      "BLE discovery complete. Found {} peripherals.",
      self.peripherals.len()
    );
    self.scanning_status.store(false, Ordering::Relaxed);
    self
      .sender
      .try_send(CustomHardwareCommunicationManagerEvent::ScanningFinished)
      .unwrap_or_else(|e| log::warn!("Failed to send scanning finished event: {:?}", e));
  }
}
