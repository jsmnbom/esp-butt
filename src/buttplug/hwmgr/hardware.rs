use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use buttplug_core::{errors::ButtplugDeviceError, util::async_manager};
use buttplug_server::device::hardware::{
  Hardware,
  HardwareConnector,
  HardwareEvent,
  HardwareInternal,
  HardwareReadCmd,
  HardwareReading,
  HardwareSpecializer,
  HardwareSubscribeCmd,
  HardwareUnsubscribeCmd,
  HardwareWriteCmd,
  communication::HardwareSpecificError,
};
use buttplug_server_device_config::{
  BluetoothLESpecifier,
  Endpoint,
  ProtocolCommunicationSpecifier,
};
use futures::{
  FutureExt,
  future::{self, BoxFuture},
};
use log::{error, trace, warn};
use tokio::sync::{Notify, broadcast, watch};
use tracing::info_span;

use crate::ble::{
  BleError,
  Characteristic,
  Client,
  ClientConnector,
  ClientEvent,
  ConnectionParameters,
  Notification,
  PeripheralProperties,
};

#[derive(Debug)]
pub struct BleHardwareConnector {
  properties: PeripheralProperties,
  rssi_notify: Arc<Notify>,
  rssi_tx: watch::Sender<i8>,
}

impl BleHardwareConnector {
  pub fn new(
    properties: &PeripheralProperties,
    rssi_notify: Arc<Notify>,
    rssi_tx: watch::Sender<i8>,
  ) -> Self {
    Self {
      properties: properties.clone(),
      rssi_notify,
      rssi_tx,
    }
  }
}

#[async_trait]
impl HardwareConnector for BleHardwareConnector {
  fn specifier(&self) -> ProtocolCommunicationSpecifier {
    ProtocolCommunicationSpecifier::BluetoothLE(BluetoothLESpecifier::new_from_device(
      &self.properties.name,
      &self
        .properties
        .manufacturer_data
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect(),
      &self.properties.services,
    ))
  }

  async fn connect(&mut self) -> Result<Box<dyn HardwareSpecializer>, ButtplugDeviceError> {
    let connector = ClientConnector::new(self.properties.address);

    let client = connector.connect().await.map_err(|e| {
      ButtplugDeviceError::DeviceConnectionError(format!(
        "Failed to connect to device {:?}: {:?}",
        self.properties.address, e
      ))
    })?;

    Ok(Box::new(BleHardwareSpecializer::new(
      self.properties.name.clone(),
      client,
      self.rssi_notify.clone(),
      self.rssi_tx.clone(),
    )))
  }
}

pub struct BleHardwareSpecializer {
  name: String,
  device: Option<Client>,
  rssi_notify: Arc<Notify>,
  rssi_tx: watch::Sender<i8>,
}

impl BleHardwareSpecializer {
  pub fn new(
    name: String,
    device: Client,
    rssi_notify: Arc<Notify>,
    rssi_tx: watch::Sender<i8>,
  ) -> Self {
    Self {
      name,
      device: Some(device),
      rssi_notify,
      rssi_tx,
    }
  }
}

#[async_trait]
impl HardwareSpecializer for BleHardwareSpecializer {
  async fn specialize(
    &mut self,
    specifiers: &[ProtocolCommunicationSpecifier],
  ) -> Result<Hardware, ButtplugDeviceError> {
    let mut endpoint_characteristic_map = Vec::new();
    let mut attr_handle_endpoint_map = Vec::new();
    let device = self.device.take().unwrap();

    if let Some(ProtocolCommunicationSpecifier::BluetoothLE(btle)) = specifiers
      .iter()
      .find(|x| matches!(x, ProtocolCommunicationSpecifier::BluetoothLE(_)))
    {
      for (proto_uuid, proto_service) in btle.services() {
        for service in device.services_iter() {
          if service.uuid != *proto_uuid {
            continue;
          }

          trace!("Found required service {} {:?}", service.uuid, service);
          for (chr_name, chr_uuid) in proto_service.iter() {
            if let Some(chr) = service.characteristics.iter().find(|c| c.uuid == *chr_uuid) {
              trace!(
                "Found characteristic {} for endpoint {}",
                chr.uuid, *chr_name
              );
              endpoint_characteristic_map.push((*chr_name, chr.clone()));
              attr_handle_endpoint_map.push((chr.value_handle, *chr_name));
            } else {
              error!(
                "Characteristic {} ({}) not found, may cause issues in connection.",
                chr_name, chr_uuid
              );
            }
          }
        }
      }
    } else {
      error!(
        "Can't find btle protocol specifier mapping for device {} {:?}",
        self.name,
        device.address()
      );
      return Err(ButtplugDeviceError::DeviceConnectionError(format!(
        "Can't find btle protocol specifier mapping for device {} {:?}",
        self.name,
        device.address()
      )));
    }

    let name = self.name.clone().to_string();
    let address = format!("{}", device.address());
    let mut endpoints_list = Vec::with_capacity(endpoint_characteristic_map.len());
    for (endpoint, _) in endpoint_characteristic_map.iter() {
      endpoints_list.push(*endpoint);
    }

    let device_internal_impl = BleHardware::new(
      device,
      self.name.clone(),
      endpoint_characteristic_map,
      attr_handle_endpoint_map,
      self.rssi_notify.clone(),
      self.rssi_tx.clone(),
    );
    Ok(Hardware::new(
      &name,
      &address,
      &endpoints_list,
      &Some(Duration::from_millis(75)),
      false,
      Box::new(device_internal_impl),
    ))
  }
}

struct BleHardware {
  #[allow(dead_code)]
  device: Client,
  name: String,
  event_stream: broadcast::Sender<HardwareEvent>,
  endpoint_characteristic_map: Vec<(Endpoint, Characteristic)>,
}

impl BleHardware {
  pub fn new(
    device: Client,
    name: String,
    endpoint_characteristic_map: Vec<(Endpoint, Characteristic)>,
    attr_handle_endpoint_map: Vec<(u16, Endpoint)>,
    rssi_notify: Arc<Notify>,
    rssi_tx: watch::Sender<i8>,
  ) -> Self {
    let (event_stream, _) = broadcast::channel(64);
    let event_stream_clone = event_stream.clone();

    let mut device_events = device.events();
    let address = device.address().to_string();
    let conn_handle = device.conn_handle();

    async_manager::spawn(
      async move {
        loop {
          tokio::select! {
            biased;

            _ = rssi_notify.notified() => {
              match crate::ble::read_rssi_for_conn(conn_handle) {
                Ok(rssi) => {
                  rssi_tx.send_replace(rssi);
                }
                Err(e) => { log::warn!("[{}] Failed to read connected RSSI: {:?}", address, e); }
              }
            }
            event = device_events.recv() => {
              match event {
                Ok(Ok(event)) => match event {
                  ClientEvent::Notification(Notification { attr_handle, data }) => {
                    log::info!("[{}] Notification: {} {:02x?} | str: {:?}", address, attr_handle, data, std::str::from_utf8(&data).ok());

                    if let Some((_, endpoint)) = attr_handle_endpoint_map
                      .iter()
                      .find(|(handle, _)| *handle == attr_handle)
                    {
                      if event_stream_clone
                        .send(HardwareEvent::Notification(
                          address.clone(),
                          endpoint.clone(),
                          data,
                        )).is_err() {
                          log::error!("[{}] Failed to send notification event for endpoint {}: channel closed", address, endpoint);
                        }
                    }
                  }
                  ClientEvent::Disconnected => {
                    log::info!("[{}] Device disconnected", address);
                    if event_stream_clone
                      .send(HardwareEvent::Disconnected(address.clone()))
                      .is_err() {
                        log::error!("[{}] Failed to send disconnected event: channel closed", address);
                      }
                  }
                  _ => {
                    log::info!("[{}] Received unknown device event: {:?}", address, event);
                  }
                },
                Ok(Err(e)) => {
                  log::error!("[{}] Error in device event: {:?}", address, e);
                }
                Err(e) => {
                  log::error!("[{}] Device event stream closed: {:?}", address, e);
                  return;
                }
              }
            }

          }
        }
      },
      info_span!("ble-hardware", address = device.address().to_string()),
    );

    Self {
      device,
      name,
      event_stream,
      endpoint_characteristic_map,
    }
  }

  fn get_characteristic(&self, endpoint: &Endpoint) -> Result<Characteristic, ButtplugDeviceError> {
    match self
      .endpoint_characteristic_map
      .iter()
      .find(|(e, _)| e == endpoint)
    {
      Some((_, chr)) => Ok(chr.clone()),
      None => {
        return Err(ButtplugDeviceError::InvalidEndpoint(endpoint.to_string()));
      }
    }
  }
}

impl HardwareInternal for BleHardware {
  fn disconnect(&self) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    let result = self.device.disconnect().map_err(to_hardware_err);
    async move { result }.boxed()
  }
  fn event_stream(&self) -> broadcast::Receiver<HardwareEvent> {
    self.event_stream.subscribe()
  }
  fn read_value(
    &self,
    msg: &HardwareReadCmd,
  ) -> BoxFuture<'static, Result<HardwareReading, ButtplugDeviceError>> {
    log::debug!("Reading value for endpoint {}", msg.endpoint());
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    let endpoint = msg.endpoint();
    async move {
      let data = characteristic.read().await.map_err(to_hardware_err)?;
      log::debug!(
        "  data: {:02x?} | str: {:?}",
        data,
        std::str::from_utf8(&data).ok()
      );
      Ok(HardwareReading::new(endpoint, &data))
    }
    .boxed()
  }
  fn write_value(
    &self,
    msg: &HardwareWriteCmd,
  ) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    log::debug!("Writing value for endpoint {}", msg.endpoint());
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    let mut with_response = msg.write_with_response();
    if with_response && !characteristic.properties.supports_write() {
      if characteristic.properties.supports_write_no_response() {
        warn!(
          "Device {} does not support write with response, falling back to write without response",
          self.name
        );
        with_response = false;
      } else {
        return err_boxed(ButtplugDeviceError::DeviceSpecificError(
          HardwareSpecificError::HardwareSpecificError(
            "ble".to_owned(),
            format!("Device {} does not support write", self.name),
          )
          .to_string(),
        ));
      }
    } else if !with_response && !characteristic.properties.supports_write_no_response() {
      if characteristic.properties.supports_write() {
        warn!(
          "Device {} does not support write without response, falling back to write with response",
          self.name
        );
        with_response = true;
      } else {
        return err_boxed(ButtplugDeviceError::DeviceSpecificError(
          HardwareSpecificError::HardwareSpecificError(
            "ble".to_owned(),
            format!("Device {} does not support write", self.name),
          )
          .to_string(),
        ));
      }
    } else if !characteristic.properties.supports_write()
      && !characteristic.properties.supports_write_no_response()
    {
      return err_boxed(ButtplugDeviceError::DeviceSpecificError(
        HardwareSpecificError::HardwareSpecificError(
          "ble".to_owned(),
          format!("Device {} does not support any write mode", self.name),
        )
        .to_string(),
      ));
    }

    let data = msg.data().clone();
    async move {
      log::debug!(
        "  data: {:02x?} | str: {:?}",
        data,
        std::str::from_utf8(&data).ok()
      );
      if with_response {
        characteristic.write(&data).await.map_err(to_hardware_err)?;
      } else {
        characteristic
          .write_no_response(&data)
          .map_err(to_hardware_err)?;
      }
      Ok(())
    }
    .boxed()
  }
  fn subscribe(
    &self,
    msg: &HardwareSubscribeCmd,
  ) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    log::debug!("Subscribing to endpoint {}", msg.endpoint());
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    async move {
      characteristic.subscribe().await.map_err(to_hardware_err)?;
      Ok(())
    }
    .boxed()
  }
  fn unsubscribe(
    &self,
    msg: &HardwareUnsubscribeCmd,
  ) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    log::debug!("Unsubscribing from endpoint {}", msg.endpoint());
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    async move {
      characteristic
        .unsubscribe()
        .await
        .map_err(to_hardware_err)?;
      Ok(())
    }
    .boxed()
  }
}

fn err_boxed<T: std::marker::Send + 'static>(
  e: ButtplugDeviceError,
) -> BoxFuture<'static, Result<T, ButtplugDeviceError>> {
  future::ready(Err(e)).boxed()
}

fn to_hardware_err(e: BleError) -> ButtplugDeviceError {
  log::error!("Device ble error: {e:?}");
  ButtplugDeviceError::DeviceSpecificError(
    HardwareSpecificError::HardwareSpecificError("ble".to_owned(), format!("{e:?}")).to_string(),
  )
}
