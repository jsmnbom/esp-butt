use std::{collections::HashMap, pin::Pin};

use async_trait::async_trait;
use buttplug_core::errors::ButtplugDeviceError;
use buttplug_server::device::hardware::{Hardware, HardwareConnector, HardwareSpecializer};
use buttplug_server_device_config::{BluetoothLESpecifier, ProtocolCommunicationSpecifier};
use uuid::Uuid;

use crate::ble;

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

    Ok(Box::new(BleHardwareSpecializer::new(client)))

  }
}


pub struct BleHardwareSpecializer {
  device: Pin<Box<ble::Client>>,
}

impl BleHardwareSpecializer {
  pub fn new(device: Pin<Box<ble::Client>>) -> Self {
    Self { device }
  }
}

#[async_trait]
impl HardwareSpecializer for BleHardwareSpecializer {
  async fn specialize(
    &mut self,
    specifiers: &[ProtocolCommunicationSpecifier],
  ) -> Result<Hardware, ButtplugDeviceError> {
    unimplemented!()
  }
}
