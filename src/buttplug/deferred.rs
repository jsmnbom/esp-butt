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
use futures::Stream;
use tokio::sync::{Notify, mpsc};
use tracing::info_span;

use crate::utils;

/// Represents a device that buttplug has matched to a known protocol and is ready to
#[derive(Debug, Clone)]
pub struct DiscoveredDevice {
  pub name: String,
  approve: Arc<Notify>,
}

impl DiscoveredDevice {
  pub fn approve(&self) {
    self.approve.notify_one();
  }
}

struct DeferredHardwareConnector {
  inner: Box<dyn HardwareConnector + Send>,
  name: String,
  discovered_tx: mpsc::Sender<DiscoveredDevice>,
}

impl std::fmt::Debug for DeferredHardwareConnector {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("DeferredHardwareConnector")
      .field("name", &self.name)
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
      approve: approve.clone(),
    };

    self.discovered_tx.send(discovered).await.map_err(|e| {
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
  discovered_tx: mpsc::Sender<DiscoveredDevice>,
}

impl<I: HardwareCommunicationManagerBuilder> DeferredCommunicationManagerBuilder<I> {
  pub fn new(inner: I) -> (Self, impl Stream<Item = DiscoveredDevice>) {
    let (tx, rx) = mpsc::channel(16);
    let stream = utils::stream::convert_mpsc_receiver_to_stream(rx);
    (Self { inner, discovered_tx: tx }, stream)
  }
}

impl<I: HardwareCommunicationManagerBuilder> HardwareCommunicationManagerBuilder
  for DeferredCommunicationManagerBuilder<I>
{
  fn finish(
    &mut self,
    sender: tokio::sync::mpsc::Sender<HardwareCommunicationManagerEvent>,
  ) -> Box<dyn HardwareCommunicationManager> {
    let (wrapper_tx, mut wrapper_rx) =
      tokio::sync::mpsc::channel::<HardwareCommunicationManagerEvent>(64);

    let discovered_tx = self.discovered_tx.clone();

    async_manager::spawn(
      async move {
        while let Some(event) = wrapper_rx.recv().await {
          let forwarded = match event {
            HardwareCommunicationManagerEvent::DeviceFound { name, address, creator } => {
              // Wrap the creator in a DeferredHardwareConnector
              HardwareCommunicationManagerEvent::DeviceFound {
                name: name.clone(),
                address,
                creator: Box::new(DeferredHardwareConnector {
                  inner: creator,
                  name,
                  discovered_tx: discovered_tx.clone(),
                }),
              }
            }
            other => other,
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
