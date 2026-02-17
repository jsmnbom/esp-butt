use std::ffi::c_void;

use allocator_api2::vec::Vec;
use bitflags::bitflags;
use bt_hci::uuid::BluetoothUuid16;
use esp_idf_svc::sys::{self, esp};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::ble;
use crate::utils::heap::ExternalMemory;
use crate::utils::os_mbuf::{self, OsMBuf};

bitflags! {
  #[repr(transparent)]
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub struct CharacteristicProperties: u8 {
    const BROADCAST = sys::BLE_GATT_CHR_PROP_BROADCAST as _;
    const READ = sys::BLE_GATT_CHR_PROP_READ as _;
    const WRITE_NO_RSP = sys::BLE_GATT_CHR_PROP_WRITE_NO_RSP as _;
    const WRITE = sys::BLE_GATT_CHR_PROP_WRITE as _;
    const NOTIFY = sys::BLE_GATT_CHR_PROP_NOTIFY as _;
    const INDICATE = sys::BLE_GATT_CHR_PROP_INDICATE as _;
    const AUTH_SIGN_WRITE = sys::BLE_GATT_CHR_PROP_AUTH_SIGN_WRITE as _;
    const EXTENDED = sys::BLE_GATT_CHR_PROP_EXTENDED as _;
  }
}

impl CharacteristicProperties {
  pub fn supports_write_no_response(&self) -> bool {
    self.contains(CharacteristicProperties::WRITE_NO_RSP)
  }

  pub fn supports_write(&self) -> bool {
    self.contains(CharacteristicProperties::WRITE)
  }
}

#[derive(Debug, Clone)]
pub struct Descriptor {
  pub uuid: Uuid,
  pub handle: u16,
}

impl Descriptor {
  pub fn new(value: &sys::ble_gatt_dsc) -> Self {
    Self {
      uuid: ble::utils::uuid_from_any_t(&value.uuid),
      handle: value.handle,
    }
  }
}

#[derive(Debug, Clone)]
pub struct Characteristic {
  conn_handle: u16,
  pub uuid: Uuid,
  pub definition_handle: u16,
  pub value_handle: u16,
  pub properties: CharacteristicProperties,
  pub descriptors: Vec<Descriptor, ExternalMemory>,
}

impl Characteristic {
  pub fn new(conn_handle: u16, value: &sys::ble_gatt_chr) -> Self {
    Self {
      conn_handle,
      uuid: ble::utils::uuid_from_any_t(&value.uuid),
      definition_handle: value.def_handle,
      value_handle: value.val_handle,
      properties: CharacteristicProperties::from_bits(value.properties).unwrap(),
      descriptors: Vec::new_in(ExternalMemory),
    }
  }

  pub fn write_no_response(&self, data: &[u8]) -> Result<(), ble::BleError> {
    write_no_response(self.conn_handle, self.value_handle, data)
  }

  pub async fn write(&self, data: &[u8]) -> Result<(), ble::BleError> {
    write_with_response(self.conn_handle, self.value_handle, data).await
  }

  pub async fn read(&self) -> Result<Vec<u8>, ble::BleError> {
    read(self.conn_handle, self.value_handle).await
  }

  pub async fn subscribe(&self) -> Result<(), ble::BleError> {
    self.set_notify(&[0x01, 0x00]).await
  }

  pub async fn unsubscribe(&self) -> Result<(), ble::BleError> {
    self.set_notify(&[0x00, 0x00]).await
  }

  pub async fn set_notify(&self, val: &[u8]) -> Result<(), ble::BleError> {
    let desc = self
      .descriptors
      .iter()
      .find(|d| d.uuid == Uuid::from(BluetoothUuid16::new(0x2902)))
      .ok_or(ble::BleError::MissingNotifyDescriptor)?;
    log::info!(
      "Setting characteristic {:?} desc {:?} with value {:?}",
      self.uuid,
      desc.uuid,
      val
    );
    write_with_response(self.conn_handle, desc.handle, val).await?;
    Ok(())
  }
}

#[derive(Debug)]
pub struct Service {
  pub uuid: Uuid,
  pub start_handle: u16,
  pub end_handle: u16,
  pub characteristics: Vec<Characteristic, ExternalMemory>,
}

impl Service {
  #[allow(dead_code)]
  pub fn get_characteristic(&self, uuid: Uuid) -> Option<&Characteristic> {
    self.characteristics.iter().find(|c| c.uuid == uuid)
  }
}

impl Service {
  pub fn new(value: &sys::ble_gatt_svc) -> Self {
    Self {
      uuid: ble::utils::uuid_from_any_t(&value.uuid),
      start_handle: value.start_handle,
      end_handle: value.end_handle,
      characteristics: Vec::new_in(ExternalMemory),
    }
  }
}

pub fn write_no_response(
  conn_handle: u16,
  attr_handle: u16,
  data: &[u8],
) -> Result<(), ble::BleError> {
  if data.len() > ble::utils::get_mtu(conn_handle) as usize {
    unsafe {
      esp!(sys::ble_gattc_write_no_rsp_flat(
        conn_handle,
        attr_handle,
        data.as_ptr() as *const c_void,
        data.len() as u16,
      ))?
    }
  } else {
    // TODO: Check if this is dropped ever
    let mbuf = os_mbuf::OsMBuf::from_slice(data).unwrap();

    unsafe {
      esp!(sys::ble_gattc_write_no_rsp(
        conn_handle,
        attr_handle,
        mbuf.as_raw()
      ))?
    }
  }
  Ok(())
}

pub async fn write_with_response(
  conn_handle: u16,
  attr_handle: u16,
  data: &[u8],
) -> Result<(), ble::BleError> {
  let (tx, rx) = oneshot::channel::<Result<(), ble::BleError>>();
  if data.len() > ble::utils::get_mtu(conn_handle) as usize {
    unsafe {
      esp!(sys::ble_gattc_write_flat(
        conn_handle,
        attr_handle,
        data.as_ptr() as *const c_void,
        data.len() as u16,
        Some(on_gatt_attr_write),
        Box::into_raw(Box::new(tx)) as *mut c_void,
      ))?
    }
  } else {
    // TODO: Check if this is dropped ever
    let mbuf = os_mbuf::OsMBuf::from_slice(data).unwrap();

    unsafe {
      esp!(sys::ble_gattc_write_long(
        conn_handle,
        attr_handle,
        0,
        mbuf.as_raw(),
        Some(on_gatt_attr_write),
        Box::into_raw(Box::new(tx)) as *mut c_void,
      ))?
    }
  }

  match rx.await {
    Ok(v) => v,
    Err(_) => Err(ble::BleError::Internal),
  }
}

pub async fn read(conn_handle: u16, attr_handle: u16) -> Result<Vec<u8>, ble::BleError> {
  let (tx, rx) = oneshot::channel::<Result<Vec<u8>, ble::BleError>>();

  unsafe {
    esp!(sys::ble_gattc_read(
      conn_handle,
      attr_handle,
      Some(on_gatt_attr_read),
      Box::into_raw(Box::new(tx)) as *mut c_void,
    ))?
  }

  match rx.await {
    Ok(v) => v,
    Err(_) => Err(ble::BleError::Internal),
  }
}

extern "C" fn on_gatt_attr_write(
  _conn_handle: u16,
  error: *const sys::ble_gatt_error,
  _attr: *mut sys::ble_gatt_attr,
  arg: *mut ::core::ffi::c_void,
) -> i32 {
  let error = unsafe { &*error };
  let arg = unsafe { Box::from_raw(arg as *mut oneshot::Sender<Result<(), ble::BleError>>) };

  if error.status == 0 {
    let _ = arg.send(Ok(()));
  } else {
    let _ = arg.send(Err(ble::BleError::NimbleError(error.status)));
  }

  0
}

extern "C" fn on_gatt_attr_read(
  _conn_handle: u16,
  error: *const sys::ble_gatt_error,
  attr: *mut sys::ble_gatt_attr,
  arg: *mut ::core::ffi::c_void,
) -> i32 {
  let error = unsafe { &*error };
  let arg = unsafe { Box::from_raw(arg as *mut oneshot::Sender<Result<Vec<u8>, ble::BleError>>) };

  if error.status == 0 {
    if error.status == 0
      && let Some(attr) = unsafe { attr.as_ref() }
    {
      let mut data = Vec::new();
      for om in OsMBuf(attr.om).iter() {
        data.extend_from_slice(om.as_slice());
      }

      let _ = arg.send(Ok(data));
    }
  } else {
    let _ = arg.send(Err(ble::BleError::NimbleError(error.status)));
  }

  0
}
