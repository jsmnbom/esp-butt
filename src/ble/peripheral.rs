use std::collections::HashMap;

use bt_hci::uuid::BluetoothUuid16;
use smallvec::SmallVec;
use uuid::Uuid;

use crate::ble;

#[derive(Debug, Clone)]
pub struct PeripheralProperties {
  pub address: ble::Address,
  pub rssi: i8,
  pub name: compact_str::CompactString,
  pub manufacturer_data: SmallVec<[(u16, SmallVec<[u8; 32]>); 1]>,
  pub services: SmallVec<[Uuid; 4]>,
}

impl PeripheralProperties {
  pub fn new(address: ble::Address, rssi: i8) -> Self {
    Self {
      address,
      rssi,
      name: compact_str::CompactString::new(""),
      manufacturer_data: SmallVec::new(),
      services: smallvec::SmallVec::new(),
    }
  }

  pub fn update(&mut self, report: &ble::AdReport) -> Result<(), ble::BleError> {
    self.rssi = report.rssi;
    for structure in report.data.iter() {
      match structure {
        ble::AdStructure::CompleteLocalName(name) => {
          if let Ok(name) = core::str::from_utf8(name) {
            self.name = name.into();
          }
        }
        ble::AdStructure::ShortenedLocalName(name) if self.name.is_empty() => {
          if let Ok(name) = core::str::from_utf8(name) {
            self.name = name.into();
          }
        }
        ble::AdStructure::ManufacturerData { company, payload } => {
          for (id, data) in &mut self.manufacturer_data {
            if *id == company {
              data.extend_from_slice(payload);
              return Ok(());
            }
          }
          self
            .manufacturer_data
            .push((company, SmallVec::from_slice(payload)));
        }
        ble::AdStructure::ServiceUuids16(uuids) => {
          for uuid in uuids {
            self.services.push(Uuid::from(bt_hci::uuid::BluetoothUuid::Uuid16(
              BluetoothUuid16::from_le_slice(uuid)
                .map_err(|_| ble::BleError::InvalidValue)?,
            )));
          }
        }
        ble::AdStructure::ServiceUuids128(uuids) => {
          for uuid in uuids {
            self.services.push(Uuid::from(bt_hci::uuid::BluetoothUuid::Uuid128(
              bt_hci::uuid::BluetoothUuid128::from_le_slice(uuid)
                .map_err(|_| ble::BleError::InvalidValue)?,
            )));
          }
        }
        _ => {}
      }
    }
    Ok(())
  }
}
