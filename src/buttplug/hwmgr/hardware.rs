use std::time::Duration;

use async_trait::async_trait;
use buttplug_core::errors::ButtplugDeviceError;
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
use compact_str::CompactString;
use futures::{
  FutureExt,
  future::{self, BoxFuture},
};
use hashbrown::{DefaultHashBuilder, HashMap};
use log::{debug, error, warn};
use tokio::sync::broadcast;

use crate::{
  ble::{self, ClientEvent},
  utils::{self, heap::ExternalMemory},
};

#[derive(Debug)]
pub struct BleHardwareConnector {
  properties: ble::PeripheralProperties,
}

impl BleHardwareConnector {
  pub fn new(properties: &ble::PeripheralProperties) -> Self {
    Self {
      properties: properties.clone(),
    }
  }
}

#[async_trait]
impl HardwareConnector for BleHardwareConnector {
  fn specifier(&self) -> ProtocolCommunicationSpecifier {
    ProtocolCommunicationSpecifier::BluetoothLE(BluetoothLESpecifier::new_from_device(
      &self.properties.name,
      &self.properties.manufacturer_data,
      &self.properties.services,
    ))
  }

  async fn connect(&mut self) -> Result<Box<dyn HardwareSpecializer>, ButtplugDeviceError> {
    let connector = ble::ClientConnector::new(self.properties.address);
    let client = connector.connect().await.map_err(|e| {
      ButtplugDeviceError::DeviceConnectionError(format!(
        "Failed to connect to device {:?}: {:?}",
        self.properties.address, e
      ))
    })?;

    Ok(Box::new(BleHardwareSpecializer::new(
      self.properties.name.clone(),
      client,
    )))
  }
}

pub struct BleHardwareSpecializer {
  name: CompactString,
  device: Option<ble::Client>,
}

impl BleHardwareSpecializer {
  pub fn new(name: CompactString, device: ble::Client) -> Self {
    Self {
      name,
      device: Some(device),
    }
  }
}

#[async_trait]
impl HardwareSpecializer for BleHardwareSpecializer {
  async fn specialize(
    &mut self,
    specifiers: &[ProtocolCommunicationSpecifier],
  ) -> Result<Hardware, ButtplugDeviceError> {
    let mut endpoint_characteristic_map = HashMap::new_in(ExternalMemory);
    let mut attr_handle_endpoint_map = HashMap::new_in(ExternalMemory);
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

          debug!("Found required service {} {:?}", service.uuid, service);
          for (chr_name, chr_uuid) in proto_service.iter() {
            if let Some(chr) = service.characteristics.iter().find(|c| c.uuid == *chr_uuid) {
              debug!(
                "Found characteristic {} for endpoint {}",
                chr.uuid, *chr_name
              );
              endpoint_characteristic_map.insert(*chr_name, chr.clone());
              attr_handle_endpoint_map.insert(chr.value_handle, *chr_name);
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
    let endpoints_list = endpoint_characteristic_map
      .keys()
      .cloned()
      .collect::<Vec<Endpoint>>();

    let device_internal_impl = BleHardware::new(
      device,
      self.name.clone(),
      endpoint_characteristic_map,
      attr_handle_endpoint_map,
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
  device: ble::Client,
  name: CompactString,
  event_stream: broadcast::Sender<HardwareEvent>,
  endpoint_characteristic_map:
    HashMap<Endpoint, ble::Characteristic, DefaultHashBuilder, ExternalMemory>,
}

impl BleHardware {
  pub fn new(
    device: ble::Client,
    name: CompactString,
    endpoint_characteristic_map: HashMap<
      Endpoint,
      ble::Characteristic,
      DefaultHashBuilder,
      ExternalMemory,
    >,
    attr_handle_endpoint_map: HashMap<u16, Endpoint, DefaultHashBuilder, ExternalMemory>,
  ) -> Self {
    let (event_stream, _) = broadcast::channel(16);
    let event_stream_clone = event_stream.clone();

    let mut device_events = device.events();
    let address = format!("{}", device.address());

    utils::spawn::spawn(
      async move {
        loop {
          match device_events.recv().await {
            Ok(Ok(event)) => match event {
              ClientEvent::Notification(ble::Notification { attr_handle, data }) => {
                if let Some(endpoint) = attr_handle_endpoint_map.get(&attr_handle) {
                  event_stream_clone
                    .send(HardwareEvent::Notification(
                      address.clone(),
                      endpoint.clone(),
                      data,
                    ))
                    .unwrap();
                }
              }
              ClientEvent::Disconnected => {
                event_stream_clone
                  .send(HardwareEvent::Disconnected(address.clone()))
                  .unwrap();
              }
              _ => {}
            },
            Ok(Err(e)) => {
              error!("Error in device event: {:?}", e);
            }
            Err(e) => {
              error!("Device event stream closed: {:?}", e);
              return;
            }
          }
        }
      },
      c"hardware",
      16 * 1024,
      utils::spawn::PRO_CORE,
    );

    Self {
      device,
      name,
      event_stream,
      endpoint_characteristic_map,
    }
  }

  fn get_characteristic(
    &self,
    endpoint: &Endpoint,
  ) -> Result<ble::Characteristic, ButtplugDeviceError> {
    match self.endpoint_characteristic_map.get(endpoint) {
      Some(chr) => Ok(chr.clone()),
      None => {
        return Err(ButtplugDeviceError::InvalidEndpoint(endpoint.to_string()));
      }
    }
  }
}

impl HardwareInternal for BleHardware {
  fn disconnect(&self) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    unimplemented!()
  }
  fn event_stream(&self) -> broadcast::Receiver<HardwareEvent> {
    self.event_stream.subscribe()
  }
  fn read_value(
    &self,
    msg: &HardwareReadCmd,
  ) -> BoxFuture<'static, Result<HardwareReading, ButtplugDeviceError>> {
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    let endpoint = msg.endpoint();
    async move {
      let data = characteristic.read().await.map_err(to_hardware_err)?;
      Ok(HardwareReading::new(endpoint, &data))
    }
    .boxed()
  }
  fn write_value(
    &self,
    msg: &HardwareWriteCmd,
  ) -> BoxFuture<'static, Result<(), ButtplugDeviceError>> {
    let characteristic = match self.get_characteristic(&msg.endpoint()) {
      Ok(chr) => chr,
      Err(e) => return err_boxed(e),
    };
    let mut with_response = msg.write_with_response();
    if with_response
      && !characteristic.properties.supports_write()
      && characteristic.properties.supports_write_no_response()
    {
      warn!(
        "Device {} does not support write with response, falling back to write without response",
        self.name
      );
      with_response = false;
    } else if !with_response
      && !characteristic.properties.supports_write_no_response()
      && characteristic.properties.supports_write()
    {
      warn!(
        "Device {} does not support write without response, falling back to write with response",
        self.name
      );
      with_response = true;
    } else {
      return err_boxed(ButtplugDeviceError::DeviceSpecificError(
        HardwareSpecificError::HardwareSpecificError(
          "ble".to_owned(),
          format!(
            "Device {} does not support write - no fallback availible",
            self.name
          ),
        )
        .to_string(),
      ));
    }

    let data = msg.data().clone();
    async move {
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

fn to_hardware_err(e: ble::BleError) -> ButtplugDeviceError {
  ButtplugDeviceError::DeviceSpecificError(
    HardwareSpecificError::HardwareSpecificError("ble".to_owned(), format!("{e:?}")).to_string(),
  )
}
