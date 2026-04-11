use btuuid::{BluetoothUuid16, BluetoothUuid32, BluetoothUuid128};
use esp_idf_svc::sys::{self, esp, esp_nofail};
use uuid::Uuid;

use crate::ble::BleError;

pub fn get_own_address_type() -> Result<u8, BleError> {
  let mut own_addr_type: u8 = 0;
  unsafe {
    esp!(sys::ble_hs_id_infer_auto(0, &mut own_addr_type))?;
  }
  Ok(own_addr_type)
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

pub fn uuid16(bytes: &[u8; 2]) -> Result<Uuid, BleError> {
  Ok(Uuid::from(
    BluetoothUuid16::from_le_slice(bytes).map_err(|_| BleError::InvalidValue)?,
  ))
}

pub fn uuid128(bytes: &[u8; 16]) -> Result<Uuid, BleError> {
  Ok(Uuid::from(
    BluetoothUuid128::from_le_slice(bytes).map_err(|_| BleError::InvalidValue)?,
  ))
}

pub fn phy_name(phy: u8) -> &'static str {
  match phy as u32 {
    sys::BLE_HCI_LE_PHY_1M => "1M",
    sys::BLE_HCI_LE_PHY_2M => "2M",
    sys::BLE_HCI_LE_PHY_CODED => "Coded",
    _ => "unknown",
  }
}

pub fn os_mbuf_to_vec(mbuf_ptr: *mut sys::os_mbuf) -> Vec<u8> {
  if mbuf_ptr.is_null() {
    return Vec::new();
  }

  let len = unsafe { sys::os_mbuf_len(mbuf_ptr) };

  let mut buf = Vec::with_capacity(len as _);

  unsafe {
    esp_nofail!(sys::ble_hs_mbuf_to_flat(
      mbuf_ptr,
      buf.as_mut_ptr() as _,
      buf.capacity() as _,
      std::ptr::null_mut()
    ));
    buf.set_len(len as _);
  }
  buf
}
