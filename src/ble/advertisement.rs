use esp_idf_svc::sys;
use strum::FromRepr;

use crate::ble;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr)]
pub enum AdEventType {
  AdvInd = sys::BLE_HCI_ADV_RPT_EVTYPE_ADV_IND as _,
  DirInd = sys::BLE_HCI_ADV_RPT_EVTYPE_DIR_IND as _,
  ScanInd = sys::BLE_HCI_ADV_RPT_EVTYPE_SCAN_IND as _,
  NonconnInd = sys::BLE_HCI_ADV_RPT_EVTYPE_NONCONN_IND as _,
  ScanRsp = sys::BLE_HCI_ADV_RPT_EVTYPE_SCAN_RSP as _,
}

pub struct AdData<'a> {
  pub(crate) payload: &'a [u8],
}

impl core::fmt::Debug for AdData<'_> {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    for byte in self.payload {
      write!(fmt, "{:02X} ", byte)?;
    }
    Ok(())
  }
}

#[allow(dead_code)]
#[derive(Debug)]
pub enum AdStructure<'a> {
  Flags(u8),
  ServiceUuids16(&'a [[u8; 2]]),
  ServiceUuids128(&'a [[u8; 16]]),
  CompleteLocalName(&'a [u8]),
  ShortenedLocalName(&'a [u8]),
  ManufacturerData { company: u16, payload: &'a [u8] },
  Unknown { ty: u8, data: &'a [u8] },
}

#[derive(Debug)]
pub struct AdReport<'a> {
  pub event_type: AdEventType,
  pub address: ble::Address,
  pub rssi: i8,
  pub data: AdData<'a>,
}

impl<'a> TryFrom<&'a sys::ble_gap_disc_desc> for AdReport<'a> {
  type Error = ble::BleError;

  fn try_from(desc: &'a sys::ble_gap_disc_desc) -> Result<Self, Self::Error> {
    let event_type = AdEventType::from_repr(desc.event_type).ok_or(ble::BleError::InvalidValue)?;
    let address = ble::Address::try_from(desc.addr)?;
    let rssi = desc.rssi;
    let data = AdData {
      payload: unsafe { core::slice::from_raw_parts(desc.data, desc.length_data as _) },
    };
    return Ok(Self {
      event_type,
      address,
      rssi,
      data,
    });
  }
}

pub struct AdStructureIter<'a> {
  payload: &'a [u8],
}

impl<'a> Iterator for AdStructureIter<'a> {
  type Item = AdStructure<'a>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.payload.len() < 2 {
      return None;
    }

    let length = (*self.payload.first()?) as usize;
    let (data, next_payload) = self.payload.split_at_checked(1 + length)?;
    self.payload = next_payload;

    if length == 0 {
      return None;
    }

    let ty = unsafe { *data.get_unchecked(1) };
    let data = data.get(2..(length + 1)).unwrap();

    let structure = match ty {
      0x01 => AdStructure::Flags(data[0]),
      0x02 | 0x03 => AdStructure::ServiceUuids16(unsafe {
        core::slice::from_raw_parts(data.as_ptr() as *const [u8; 2], data.len() / 2)
      }),
      0x06 | 0x07 => AdStructure::ServiceUuids128(unsafe {
        core::slice::from_raw_parts(data.as_ptr() as *const [u8; 16], data.len() / 16)
      }),
      0x08 => AdStructure::ShortenedLocalName(data),
      0x09 => AdStructure::CompleteLocalName(data),
      0xFF => {
        if data.len() < 2 {
          return None;
        }
        let company = u16::from_le_bytes([data[0], data[1]]);
        let payload = &data[2..];
        AdStructure::ManufacturerData { company, payload }
      }
      _ => AdStructure::Unknown { ty, data },
    };

    Some(structure)
  }
}

impl<'a> AdData<'a> {
  pub fn iter(&self) -> AdStructureIter<'a> {
    AdStructureIter {
      payload: self.payload,
    }
  }
}
