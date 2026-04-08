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
#[cfg(target_os = "espidf")]
use tokio::sync::watch;
use tokio::sync::{Notify, mpsc};
use tracing::info_span;

use crate::buttplug::backdoor::{ButtplugBackdoorEvent, DiscoveredDevice};

#[cfg(target_os = "espidf")]
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

#[cfg(target_os = "espidf")]
type InnerManagerEvent = CustomHardwareCommunicationManagerEvent;
#[cfg(not(target_os = "espidf"))]
type InnerManagerEvent = HardwareCommunicationManagerEvent;

pub trait CustomHardwareCommunicationManagerBuilder: Send {
  fn finish(
    &mut self,
    sender: mpsc::Sender<InnerManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager>;
}

#[cfg(not(target_os = "espidf"))]
impl<T: HardwareCommunicationManagerBuilder> CustomHardwareCommunicationManagerBuilder for T {
  fn finish(
    &mut self,
    sender: mpsc::Sender<HardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager> {
    HardwareCommunicationManagerBuilder::finish(self, sender)
  }
}

struct DeferredHardwareConnector {
  inner: Box<dyn HardwareConnector + Send>,
  name: String,
  address: String,
  #[cfg(target_os = "espidf")]
  rssi_rx: Option<watch::Receiver<i8>>,
  #[cfg(target_os = "espidf")]
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

    #[cfg(target_os = "espidf")]
    let (rssi_rx, rssi_notify) = (self.rssi_rx.clone(), self.rssi_notify.clone());

    #[cfg(not(target_os = "espidf"))]
    let (rssi_rx, rssi_notify) = (None, Arc::new(Notify::new()));

    let discovered = DiscoveredDevice {
      name: self.name.clone(),
      address: self.address.clone(),
      rssi_rx,
      rssi_notify,
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
    let (wrapper_tx, mut wrapper_rx) = tokio::sync::mpsc::channel::<InnerManagerEvent>(64);

    let backdoor_tx = self.backdoor_tx.clone();

    async_manager::spawn(
      async move {
        while let Some(event) = wrapper_rx.recv().await {
          let forwarded = match event {
            #[cfg(target_os = "espidf")]
            InnerManagerEvent::DeviceFound {
              name,
              address,
              rssi_rx,
              rssi_notify,
              creator,
            } => HardwareCommunicationManagerEvent::DeviceFound {
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
            },
            #[cfg(not(target_os = "espidf"))]
            InnerManagerEvent::DeviceFound {
              name,
              address,
              creator,
            } => HardwareCommunicationManagerEvent::DeviceFound {
              name: name.clone(),
              address: extract_mac_from_btleplug_address(&address),
              creator: Box::new(DeferredHardwareConnector {
                inner: creator,
                name,
                address: extract_mac_from_btleplug_address(&address),
                backdoor_tx: backdoor_tx.clone(),
              }),
            },
            InnerManagerEvent::ScanningFinished => {
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

#[cfg(not(target_os = "espidf"))]
fn extract_mac_from_btleplug_address(address: &str) -> String {
  use regex_lite::Regex;
  // PeripheralId(DeviceId { object_path: Path(\"/org/bluez/hci0/dev_E2_36_8E_D4_BA_71\\0\") }) -> E2:36:8E:D4:BA:71
  let re = Regex::new(r"([0-9A-F]{2}_[0-9A-F]{2}_[0-9A-F]{2}_[0-9A-F]{2}_[0-9A-F]{2}_[0-9A-F]{2})")
    .unwrap();
  re.captures(address)
    .and_then(|caps| caps.get(1).map(|m| m.as_str().replace('_', ":")))
    .unwrap_or_else(|| address.to_string())
}
