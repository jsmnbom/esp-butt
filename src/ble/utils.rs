use bt_hci::uuid::{BluetoothUuid16, BluetoothUuid32, BluetoothUuid128};
use esp_idf_svc::sys::{self, esp};
use uuid::Uuid;

use crate::ble;

pub fn get_own_address_type() -> Result<u8, ble::BleError> {
  let mut own_addr_type: u8 = 0;
  unsafe {
    esp!(sys::ble_hs_id_infer_auto(0, &mut own_addr_type))?;
  }
  Ok(own_addr_type)
}

pub fn get_mtu(conn_handle: u16) -> u16 {
  unsafe { sys::ble_att_mtu(conn_handle) }
}

pub fn uuid_from_any_t(uuid: &sys::ble_uuid_any_t) -> Uuid {
  unsafe {
    match uuid.u.type_ as _ {
      sys::BLE_UUID_TYPE_16 => Uuid::from(BluetoothUuid16::new(uuid.u16_.value)),
      sys::BLE_UUID_TYPE_32 => Uuid::from(BluetoothUuid32::new(uuid.u32_.value)),
      sys::BLE_UUID_TYPE_128 => Uuid::from(BluetoothUuid128::from_le_bytes(uuid.u128_.value)),
      _ => unimplemented!(),
    }
  }
}

pub fn uuid16(bytes: &[u8; 2]) -> Result<Uuid, ble::BleError> {
  Ok(Uuid::from(
    BluetoothUuid16::from_le_slice(bytes).map_err(|_| ble::BleError::InvalidValue)?,
  ))
}

pub fn uuid128(bytes: &[u8; 16]) -> Result<Uuid, ble::BleError> {
  Ok(Uuid::from(
    BluetoothUuid128::from_le_slice(bytes).map_err(|_| ble::BleError::InvalidValue)?,
  ))
}
