use std::ffi::c_void;

use bitflags::bitflags;
use btuuid::BluetoothUuid16;
use esp_idf_svc::sys::{self, esp};
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::ble::{BleError, utils};

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
      uuid: utils::uuid_from_any_t(&value.uuid),
      handle: value.handle,
    }
  }
}

#[derive(Debug, Clone)]
pub struct Characteristic {
  pub conn_handle: u16,
  pub uuid: Uuid,
  pub definition_handle: u16,
  pub value_handle: u16,
  pub properties: CharacteristicProperties,
  pub descriptors: Vec<Descriptor>,
}

impl Characteristic {
  pub fn new(conn_handle: u16, value: &sys::ble_gatt_chr) -> Self {
    Self {
      conn_handle,
      uuid: utils::uuid_from_any_t(&value.uuid),
      definition_handle: value.def_handle,
      value_handle: value.val_handle,
      properties: CharacteristicProperties::from_bits(value.properties).unwrap(),
      descriptors: Vec::new(),
    }
  }

  pub fn write_no_response(&self, data: &[u8]) -> Result<(), BleError> {
    log::trace!(
      "Writing no response to characteristic {:?} with value {:?}",
      self.uuid,
      data
    );
    write_no_response(self.conn_handle, self.value_handle, data)
  }

  pub async fn write(&self, data: &[u8]) -> Result<(), BleError> {
    log::trace!(
      "Writing to characteristic {:?} with value {:?}",
      self.uuid,
      data
    );
    write_with_response(self.conn_handle, self.value_handle, data).await
  }

  pub async fn read(&self) -> Result<Vec<u8>, BleError> {
    log::trace!("Reading from characteristic {:?}", self.uuid);
    read(self.conn_handle, self.value_handle).await
  }

  pub async fn subscribe(&self) -> Result<(), BleError> {
    log::trace!("Subscribing to characteristic {:?}", self.uuid);
    self.set_notify(&[0x01, 0x00]).await
  }

  pub async fn unsubscribe(&self) -> Result<(), BleError> {
    log::trace!("Unsubscribing from characteristic {:?}", self.uuid);
    self.set_notify(&[0x00, 0x00]).await
  }

  pub async fn set_notify(&self, val: &[u8]) -> Result<(), BleError> {
    let desc = self
      .descriptors
      .iter()
      .find(|d| d.uuid == Uuid::from(BluetoothUuid16::new(0x2902)))
      .ok_or(BleError::MissingNotifyDescriptor)?;
    log::trace!(
      "Setting characteristic {:?} desc {:?} with value {:?}",
      self.uuid,
      desc.uuid,
      val
    );
    match write_with_response(self.conn_handle, desc.handle, val).await {
      Ok(_) => Ok(()),
      Err(e) => {
        log::error!(
          "Failed to set notify for characteristic {:?}: {:?}",
          self.uuid,
          e
        );
        Err(e)
      }
    }
  }
}

#[derive(Debug)]
pub struct Service {
  pub uuid: Uuid,
  pub start_handle: u16,
  pub end_handle: u16,
  pub characteristics: Vec<Characteristic>,
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
      uuid: utils::uuid_from_any_t(&value.uuid),
      start_handle: value.start_handle,
      end_handle: value.end_handle,
      characteristics: Vec::new(),
    }
  }
}

pub fn write_no_response(conn_handle: u16, attr_handle: u16, data: &[u8]) -> Result<(), BleError> {
  unsafe {
    esp!(sys::ble_gattc_write_no_rsp_flat(
      conn_handle,
      attr_handle,
      data.as_ptr() as *const c_void,
      data.len() as u16,
    ))?
  }
  Ok(())
}

pub async fn write_with_response(
  conn_handle: u16,
  attr_handle: u16,
  data: &[u8],
) -> Result<(), BleError> {
  let (tx, rx) = oneshot::channel::<Result<(), BleError>>();
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

  match rx.await {
    Ok(v) => v,
    Err(_) => Err(BleError::Internal),
  }
}

pub async fn read(conn_handle: u16, attr_handle: u16) -> Result<Vec<u8>, BleError> {
  let (tx, rx) = oneshot::channel::<Result<Vec<u8>, BleError>>();

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
    Err(_) => Err(BleError::Internal),
  }
}

extern "C" fn on_gatt_attr_write(
  _conn_handle: u16,
  error: *const sys::ble_gatt_error,
  _attr: *mut sys::ble_gatt_attr,
  arg: *mut ::core::ffi::c_void,
) -> i32 {
  let error = unsafe { &*error };
  let arg = unsafe { Box::from_raw(arg as *mut oneshot::Sender<Result<(), BleError>>) };

  log::trace!("GATT write completed with status: {}", error.status);

  if error.status == 0 {
    let _ = arg.send(Ok(()));
  } else {
    let _ = arg.send(Err(BleError::NimbleError(error.status)));
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
  let arg = unsafe { Box::from_raw(arg as *mut oneshot::Sender<Result<Vec<u8>, BleError>>) };

  if error.status == 0 {
    if error.status == 0
      && let Some(attr) = unsafe { attr.as_ref() }
    {
      let data = utils::os_mbuf_to_vec(attr.om);

      let _ = arg.send(Ok(data));
    }
  } else {
    let _ = arg.send(Err(BleError::NimbleError(error.status)));
  }

  0
}
