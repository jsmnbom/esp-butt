use uuid::Uuid;

use crate::ble::{AdReport, AdStructure, Address, BleError, utils};

#[derive(Debug, Clone)]
pub struct PeripheralProperties {
  pub address: Address,
  pub rssi: i8,
  pub name: compact_str::CompactString,
  pub manufacturer_data: Vec<(u16, Vec<u8>)>,
  pub services: Vec<Uuid>,
}

impl PeripheralProperties {
  pub fn new(address: Address, rssi: i8) -> Self {
    Self {
      address,
      rssi,
      name: compact_str::CompactString::new(""),
      manufacturer_data: Vec::new(),
      services: Vec::new(),
    }
  }

  pub fn update(&mut self, report: &AdReport) -> Result<(), BleError> {
    self.rssi = report.rssi;
    for structure in report.data.iter() {
      match structure {
        AdStructure::CompleteLocalName(name) => {
          if let Ok(name) = core::str::from_utf8(name) {
            self.name = name.into();
          }
        }
        AdStructure::ShortenedLocalName(name) if self.name.is_empty() => {
          if let Ok(name) = core::str::from_utf8(name) {
            self.name = name.into();
          }
        }
        AdStructure::ManufacturerData { company, payload } => {
          for (id, data) in &mut self.manufacturer_data {
            if *id == company {
              data.extend_from_slice(payload);
              return Ok(());
            }
          }
          let mut data = Vec::new();
          data.extend_from_slice(payload);
          self.manufacturer_data.push((company, data));
        }
        AdStructure::ServiceUuids16(uuids) => {
          for uuid in uuids {
            self.services.push(utils::uuid16(uuid)?);
          }
        }
        AdStructure::ServiceUuids128(uuids) => {
          for uuid in uuids {
            self.services.push(utils::uuid128(uuid)?);
          }
        }
        _ => {}
      }
    }
    Ok(())
  }
}
