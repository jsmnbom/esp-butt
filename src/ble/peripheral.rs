use tokio::sync::watch;
use uuid::Uuid;

use crate::ble::{AdReport, AdStructure, Address, BleError, utils};

#[derive(Debug, Clone)]
pub struct PeripheralProperties {
  pub address: Address,
  pub name: String,
  pub manufacturer_data: Vec<(u16, Vec<u8>)>,
  pub services: Vec<Uuid>,
  rssi_tx: watch::Sender<i8>,
  rssi_rx: Option<watch::Receiver<i8>>,
}

impl PeripheralProperties {
  pub fn new(report: &AdReport) -> Self {
    let (rssi_tx, rssi_rx) = watch::channel(report.rssi);
    Self {
      address: report.address,
      name: String::new(),
      manufacturer_data: Vec::new(),
      services: Vec::new(),
      rssi_tx,
      rssi_rx: Some(rssi_rx),
    }
  }

  pub fn take_rssi_receiver(&mut self) -> Option<watch::Receiver<i8>> {
    self.rssi_rx.take()
  }

  pub fn rssi_sender(&self) -> watch::Sender<i8> {
    self.rssi_tx.clone()
  }

  pub fn update(&mut self, report: &AdReport) -> Result<bool, BleError> {
    let _ = self.rssi_tx.send_replace(report.rssi);
    let mut updated = false;
    for structure in report.data.iter() {
      match structure {
        AdStructure::CompleteLocalName(name) => {
          if let Ok(name) = core::str::from_utf8(name) {
            if self.name != name {
              self.name = name.into();
              updated = true;
            }
          }
        }
        AdStructure::ShortenedLocalName(name) if self.name.is_empty() => {
          if let Ok(name) = core::str::from_utf8(name) {
            self.name = name.into();
            updated = true;
          }
        }
        AdStructure::ManufacturerData { company, payload } => {
          if let Some((_, data)) = self
            .manufacturer_data
            .iter_mut()
            .find(|(id, _)| *id == company)
          {
            if !data.ends_with(payload) {
              data.extend_from_slice(payload);
              updated = true;
            }
          } else {
            let mut data = Vec::new();
            data.extend_from_slice(payload);
            self.manufacturer_data.push((company, data));
            updated = true;
          }
        }
        AdStructure::ServiceUuids16(uuids) => {
          for uuid in uuids {
            let uuid = utils::uuid16(uuid)?;
            if !self.services.contains(&uuid) {
              updated = true;
              self.services.push(uuid);
            }
          }
        }
        AdStructure::ServiceUuids128(uuids) => {
          for uuid in uuids {
            let uuid = utils::uuid128(uuid)?;
            if !self.services.contains(&uuid) {
              updated = true;
              self.services.push(uuid);
            }
          }
        }
        _ => {}
      }
    }
    Ok(updated)
  }
}
