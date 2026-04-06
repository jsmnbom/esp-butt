use std::sync::Arc;

use buttplug_server::{ButtplugServer, ButtplugServerBuilder, device::ServerDeviceManagerBuilder};
use futures::Stream;
use tokio::sync::mpsc;
use tracing::instrument;

use crate::{
  buttplug::{
    async_manager::EspAsyncManager,
    backdoor::ButtplugBackdoorEvent,
    connector::SimpleInProcessClientConnector,
  },
  utils::stream::convert_mpsc_receiver_to_stream,
};

pub mod async_manager;
pub mod backdoor;
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

#[instrument]
pub fn create_buttplug() -> anyhow::Result<(
  Arc<ButtplugServer>,
  SimpleInProcessClientConnector,
  impl Stream<Item = ButtplugBackdoorEvent>,
)> {
  let dcm = data::ButtplugData::load()?;

  let mut device_manager_builder = ServerDeviceManagerBuilder::new(dcm);

  let (backdoor_tx, backdoor_rx) = mpsc::channel(16);

  #[cfg(target_os = "espidf")]
  device_manager_builder.comm_manager(deferred::DeferredCommunicationManagerBuilder::new(
    hwmgr::BleCommunicationManagerBuilder::default(),
    backdoor_tx.clone(),
  ));

  #[cfg(not(target_os = "espidf"))]
  device_manager_builder.comm_manager(deferred::DeferredCommunicationManagerBuilder::new(
    buttplug_server_hwmgr_btleplug::BtlePlugCommunicationManagerBuilder::default(),
    backdoor_tx.clone(),
  ));

  let device_manager = device_manager_builder.finish()?;

  let server = Arc::new(ButtplugServerBuilder::new(device_manager).finish()?);
  let connector = SimpleInProcessClientConnector::new(server.clone());

  let backdoor_stream = convert_mpsc_receiver_to_stream(backdoor_rx);

  Ok((server, connector, backdoor_stream))
}
