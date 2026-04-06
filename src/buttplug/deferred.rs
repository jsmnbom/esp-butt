use std::sync::Arc;

use async_trait::async_trait;
use buttplug_core::{errors::ButtplugDeviceError, util::async_manager};
use buttplug_server::device::hardware::{
  HardwareConnector,
  HardwareSpecializer,
  communication::{
    HardwareCommunicationManager,
    HardwareCommunicationManagerBuilder,
    HardwareCommunicationManagerEvent,
  },
};
use tokio::sync::{Notify, mpsc, watch};
use tracing::info_span;

use crate::{
  buttplug::backdoor::{ButtplugBackdoorEvent, DiscoveredDevice},
  utils,
};

#[derive(Debug)]
pub enum CustomHardwareCommunicationManagerEvent {
  DeviceFound {
    name: String,
    address: String,
    rssi_rx: Option<watch::Receiver<i8>>,
    rssi_notify: std::sync::Arc<tokio::sync::Notify>,
    creator: Box<dyn HardwareConnector>,
  },
  ScanningFinished,
}

pub trait CustomHardwareCommunicationManagerBuilder: Send {
  fn finish(
    &mut self,
    sender: mpsc::Sender<CustomHardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager>;
}

struct DeferredHardwareConnector {
  inner: Box<dyn HardwareConnector + Send>,
  name: String,
  address: String,
  rssi_rx: Option<watch::Receiver<i8>>,
  rssi_notify: std::sync::Arc<tokio::sync::Notify>,
  backdoor_tx: mpsc::Sender<ButtplugBackdoorEvent>,
}

impl std::fmt::Debug for DeferredHardwareConnector {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("DeferredHardwareConnector")
      .field("name", &self.name)
      .field("address", &self.address)
      .finish()
  }
}

#[async_trait]
impl HardwareConnector for DeferredHardwareConnector {
  fn specifier(&self) -> buttplug_server_device_config::ProtocolCommunicationSpecifier {
    self.inner.specifier()
  }

  async fn connect(&mut self) -> Result<Box<dyn HardwareSpecializer>, ButtplugDeviceError> {
    let approve = Arc::new(Notify::new());
    let discovered = DiscoveredDevice {
      name: self.name.clone(),
      address: self.address.clone(),
      rssi_rx: self.rssi_rx.take(),
      rssi_notify: self.rssi_notify.clone(),
      approve: approve.clone(),
    };

    self
      .backdoor_tx
      .send(ButtplugBackdoorEvent::DeviceDiscovered(discovered))
      .await
      .map_err(|e| {
        ButtplugDeviceError::DeviceConnectionError(format!(
          "Failed to notify app of pending device '{}': {:?}",
          self.name, e
        ))
      })?;

    // Pause here until the user selects this device in the UI.
    approve.notified().await;

    self.inner.connect().await
  }
}

/// Wraps any `HardwareCommunicationManagerBuilder` so that discovered devices
/// are not connected until the user explicitly selects them.
pub struct DeferredCommunicationManagerBuilder<I> {
  inner: I,
  backdoor_tx: mpsc::Sender<ButtplugBackdoorEvent>,
}

impl<I: CustomHardwareCommunicationManagerBuilder> DeferredCommunicationManagerBuilder<I> {
  pub fn new(inner: I, backdoor_tx: mpsc::Sender<ButtplugBackdoorEvent>) -> Self {
    Self { inner, backdoor_tx }
  }
}

impl<I: CustomHardwareCommunicationManagerBuilder> HardwareCommunicationManagerBuilder
  for DeferredCommunicationManagerBuilder<I>
{
  fn finish(
    &mut self,
    sender: tokio::sync::mpsc::Sender<HardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager> {
    let (wrapper_tx, mut wrapper_rx) =
      tokio::sync::mpsc::channel::<CustomHardwareCommunicationManagerEvent>(64);

    let backdoor_tx = self.backdoor_tx.clone();

    async_manager::spawn(
      async move {
        while let Some(event) = wrapper_rx.recv().await {
          let forwarded = match event {
            CustomHardwareCommunicationManagerEvent::DeviceFound {
              name,
              address,
              rssi_rx,
              rssi_notify,
              creator,
            } => {
              // Wrap the creator in a DeferredHardwareConnector
              HardwareCommunicationManagerEvent::DeviceFound {
                name: name.clone(),
                address: address.clone(),
                creator: Box::new(DeferredHardwareConnector {
                  inner: creator,
                  name,
                  address,
                  rssi_rx,
                  rssi_notify,
                  backdoor_tx: backdoor_tx.clone(),
                }),
              }
            }
            CustomHardwareCommunicationManagerEvent::ScanningFinished => {
              HardwareCommunicationManagerEvent::ScanningFinished
            }
          };
          if sender.send(forwarded).await.is_err() {
            break;
          }
        }
      },
      info_span!("deferred-comm"),
    );

    self.inner.finish(wrapper_tx)
  }
}
