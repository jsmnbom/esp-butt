use std::sync::Arc;

use buttplug_server::{ButtplugServer, ButtplugServerBuilder, device::ServerDeviceManagerBuilder};
use futures::Stream;

use crate::buttplug::{async_manager::EspAsyncManager, connector::SimpleInProcessClientConnector};

pub mod async_manager;
pub mod connector;
pub mod data;
pub mod deferred;

#[cfg(target_os = "espidf")]
mod hwmgr;

pub fn init() {
  log::info!("Initializing buttplug.io async manager...");
  buttplug_core::util::async_manager::set_global_async_manager(
    Box::new(EspAsyncManager::default()),
  );
}

#[cfg(target_os = "espidf")]
type CommunicationManager = hwmgr::BleCommunicationManagerBuilder;
#[cfg(not(target_os = "espidf"))]
type CommunicationManager = buttplug_server_hwmgr_btleplug::BtlePlugCommunicationManagerBuilder;

pub fn create_buttplug() -> anyhow::Result<(
  Arc<ButtplugServer>,
  SimpleInProcessClientConnector,
  impl Stream<Item = deferred::DiscoveredDevice>,
)> {
  log::debug!("Loading Buttplug data...");

  let dcm = data::ButtplugData::load()?.finish()?;

  log::info!(
    "Loaded {} communication specifiers",
    dcm.base_communication_specifiers().len()
  );
  log::info!(
    "Loaded {} device definitions",
    dcm.base_device_definitions().len()
  );

  log::debug!("Creating device manager...");
  let mut device_manager_builder = ServerDeviceManagerBuilder::new(dcm);
  let (deferred_builder, discovery_stream) =
    deferred::DeferredCommunicationManagerBuilder::new(CommunicationManager::default());
  device_manager_builder.comm_manager(deferred_builder);
  let device_manager = device_manager_builder.finish()?;

  log::debug!("Creating server...");
  let server = Arc::new(ButtplugServerBuilder::new(device_manager).finish()?);

  log::debug!("Creating connector...");
  let connector = SimpleInProcessClientConnector::new(server.clone());

  Ok((server, connector, discovery_stream))
}
