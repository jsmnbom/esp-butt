use buttplug_client::ButtplugClient;
use buttplug_client_in_process::{
  ButtplugInProcessClientConnector,
  ButtplugInProcessClientConnectorBuilder,
};
use buttplug_server::{ButtplugServerBuilder, device::ServerDeviceManagerBuilder};
use log::info;

use crate::buttplug::async_manager::EspAsyncManager;

pub mod async_manager;
pub mod data;
pub mod hwmgr;

pub fn init() {
  info!("Initializing buttplug.io async manager...");
  buttplug_core::util::async_manager::set_global_async_manager(
    Box::new(EspAsyncManager::default()),
  );
}

pub fn create_buttplug() -> anyhow::Result<(ButtplugInProcessClientConnector, ButtplugClient)> {
  info!("Loading Buttplug data...");

  let dcm = data::load_buttplug_data()?;

  info!(
    "Loaded {} communication specifiers",
    dcm.base_communication_specifiers().len()
  );
  info!(
    "Loaded {} device definitions",
    dcm.base_device_definitions().len()
  );

  info!("Creating device manager...");
  let mut device_manager_builder = ServerDeviceManagerBuilder::new(dcm);
  device_manager_builder
    .comm_manager(hwmgr::comm_manager::BleCommunicationManagerBuilder::default());
  let device_manager = device_manager_builder.finish()?;

  info!("Creating server...");
  let server = ButtplugServerBuilder::new(device_manager).finish()?;

  info!("Creating connector...");
  let mut connector_builder = ButtplugInProcessClientConnectorBuilder::default();
  connector_builder.server(server);
  let connector = connector_builder.finish();

  info!("Creating client...");
  let client = buttplug_client::ButtplugClient::new("esp");

  Ok((connector, client))
}
