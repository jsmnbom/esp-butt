use esp_idf_svc::sys::{self, esp};

use crate::ble;

pub fn get_own_address_type() -> Result<u8, ble::BleError> {
  let mut own_addr_type: u8 = 0;
  unsafe {
    esp!(sys::ble_hs_id_infer_auto(0, &mut own_addr_type))?;
  }
  Ok(own_addr_type)
}

#[inline]
pub(crate) unsafe fn as_void_ptr<T>(r: &mut T) -> *mut ::core::ffi::c_void {
  (r as *mut T).cast()
}
