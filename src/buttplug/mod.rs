use buttplug_client_in_process::{
  ButtplugInProcessClientConnector,
  ButtplugInProcessClientConnectorBuilder,
};
use buttplug_server::{ButtplugServerBuilder, device::ServerDeviceManagerBuilder};

use crate::buttplug::async_manager::EspAsyncManager;

pub mod async_manager;
pub mod data;

#[cfg(target_os = "espidf")]
mod hwmgr;

pub fn init() {
  log::info!("Initializing buttplug.io async manager...");
  buttplug_core::util::async_manager::set_global_async_manager(
    Box::new(EspAsyncManager::default()),
  );
}

pub fn create_buttplug() -> anyhow::Result<ButtplugInProcessClientConnector> {
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

  #[cfg(target_os = "espidf")]
  device_manager_builder
    .comm_manager(hwmgr::BleCommunicationManagerBuilder::default());

  #[cfg(not(target_os = "espidf"))]
  device_manager_builder
    .comm_manager(buttplug_server_hwmgr_btleplug::BtlePlugCommunicationManagerBuilder::default());

  let device_manager = device_manager_builder.finish()?;

  log::debug!("Creating server...");
  let server = ButtplugServerBuilder::new(device_manager).finish()?;

  log::debug!("Creating connector...");
  let mut connector_builder = ButtplugInProcessClientConnectorBuilder::default();
  connector_builder.server(server);
  let connector = connector_builder.finish();

  Ok(connector)
}
