use std::ffi::c_void;
use std::marker::PhantomPinned;
use std::pin::Pin;

use allocator_api2::vec::Vec;
use esp_idf_svc::sys::{self, esp};
use tokio::sync::oneshot;

use crate::ble;
use crate::utils::heap::ExternalMemory;

#[derive(Debug)]
pub struct Peer {
  pub services: Vec<ble::Service, ExternalMemory>,

  current_service_start_handle: u16,
  current_characteristic_start_handle: u16,

  tx: Option<oneshot::Sender<Result<(), ble::BleError>>>,

  _pin: PhantomPinned,
}

impl Peer {
  pub async fn discover_services(
    conn_handle: u16,
  ) -> Result<Vec<ble::Service, ExternalMemory>, ble::BleError> {
    let (tx, rx) = oneshot::channel();
    let mut peer = Box::pin(Self {
      services: Vec::new_in(ExternalMemory),
      current_service_start_handle: 0,
      current_characteristic_start_handle: 0,
      tx: Some(tx),
      _pin: PhantomPinned,
    });

    unsafe { peer.as_mut().get_unchecked_mut() }.start_discover_services(conn_handle);

    match rx.await {
      Ok(Ok(_)) => {
        // Move services out of peer and drop peer
        Ok(unsafe { Pin::into_inner_unchecked(peer) }.into_services())
      }
      Ok(Err(e)) => Err(e),
      Err(_) => Err(ble::BleError::ConnectionFailed),
    }
  }

  fn as_raw(&self) -> *mut c_void {
    self as *const Self as *mut c_void
  }

  fn into_services(self) -> Vec<ble::Service, ExternalMemory> {
    self.services
  }

  fn fail(&mut self, error: ble::BleError) {
    log::error!("Peer discovery failed: {:?}", error);
    let _ = self.tx.take().unwrap().send(Err(error));
  }

  fn complete(&mut self) {
    log::info!("Peer discovery complete");
    let _ = self.tx.take().unwrap().send(Ok(()));
  }

  fn start_discover_services(&mut self, conn_handle: u16) {
    log::info!("Discovering services");
    unsafe {
      if let Err(error) = esp!(sys::ble_gattc_disc_all_svcs(
        conn_handle,
        Some(Self::on_gatt_disc_svc),
        self.as_raw(),
      )) {
        self.fail(error.into());
      }
    }
  }

  fn discover_characteristics(&mut self, conn_handle: u16) {
    for service in &self.services {
      if self.current_service_start_handle < service.start_handle {
        log::debug!("Discovering characteristics for service {:?}", service.uuid);
        self.current_service_start_handle = service.start_handle;

        // If start == end then there is no space for characteristics
        if service.start_handle == service.end_handle {
          continue;
        }

        unsafe {
          if let Err(error) = esp!(sys::ble_gattc_disc_all_chrs(
            conn_handle,
            service.start_handle,
            service.end_handle,
            Some(Self::on_gatt_chr),
            self.as_raw(),
          )) {
            self.fail(error.into());
          }
        }
        return;
      }
    }
    self.complete();
  }

  fn discover_descriptors(&mut self, conn_handle: u16) {
    let service = self
      .services
      .iter()
      .find(|s| s.start_handle == self.current_service_start_handle)
      .unwrap();
    for (i, characteristic) in service.characteristics.iter().enumerate() {
      if self.current_characteristic_start_handle < characteristic.definition_handle {
        log::debug!(
          "Discovering descriptors for characteristic {:?}",
          characteristic.uuid
        );
        self.current_characteristic_start_handle = characteristic.value_handle;

        // End handle is start of next characteristic or end of service
        let end_handle = if i + 1 < service.characteristics.len() {
          service.characteristics[i + 1].definition_handle - 1
        } else {
          service.end_handle
        };

        // If the characteristic has no descriptors, skip to the next characteristic
        if characteristic.value_handle == end_handle {
          self.current_characteristic_start_handle = characteristic.value_handle;
          continue;
        }

        unsafe {
          if let Err(error) = esp!(sys::ble_gattc_disc_all_dscs(
            conn_handle,
            characteristic.value_handle,
            end_handle,
            Some(Self::on_gatt_dsc),
            self.as_raw(),
          )) {
            self.fail(error.into());
          }
        }
        return;
      }
    }
    self.discover_characteristics(conn_handle);
  }
}

impl Peer {
  extern "C" fn on_gatt_disc_svc(
    conn_handle: u16,
    error: *const sys::ble_gatt_error,
    service: *const sys::ble_gatt_svc,
    arg: *mut c_void,
  ) -> i32 {
    let peer: &mut Peer = unsafe { &mut *(arg as *mut Peer) };
    let error = unsafe { &*error };

    if error.status == sys::BLE_HS_EDONE as u16 {
      log::debug!(
        "Service discovery finished, found {} services",
        peer.services.len()
      );
      // All services discovered, start discovery of characteristics
      peer.discover_characteristics(conn_handle);
      return 0;
    }

    if error.status == 0 {
      let service = unsafe { &(*service) };
      let svc = ble::Service::new(service);
      log::debug!(
        "Discovered service: {:?} ({} - {})",
        svc.uuid,
        svc.start_handle,
        svc.end_handle
      );
      peer.services.push(svc);
      return 0;
    }

    peer.fail(error.status.into());
    0
  }

  extern "C" fn on_gatt_chr(
    conn_handle: u16,
    error: *const sys::ble_gatt_error,
    chr: *const sys::ble_gatt_chr,
    arg: *mut c_void,
  ) -> i32 {
    let peer: &mut Peer = unsafe { &mut *(arg as *mut Peer) };
    let error = unsafe { &*error };

    if error.status == sys::BLE_HS_EDONE as u16 {
      // All characteristics discovered, start discovery of descriptors for this service
      peer.discover_descriptors(conn_handle);
      return 0;
    }

    if error.status == 0 {
      let chr = unsafe { &(*chr) };
      for service in &mut peer.services {
        if service.start_handle == peer.current_service_start_handle {
          let characteristic = ble::Characteristic::new(conn_handle, chr);
          log::debug!(
            "Discovered characteristic: {:?} ({}, {})",
            characteristic.uuid,
            characteristic.definition_handle,
            characteristic.value_handle
          );
          service.characteristics.push(characteristic);
          return 0;
        }
      }
      return 0;
    }

    peer.fail(error.status.into());
    0
  }

  extern "C" fn on_gatt_dsc(
    conn_handle: u16,
    error: *const sys::ble_gatt_error,
    chr_val_handle: u16,
    dsc: *const sys::ble_gatt_dsc,
    arg: *mut c_void,
  ) -> i32 {
    let peer: &mut Peer = unsafe { &mut *(arg as *mut Peer) };
    let error = unsafe { &*error };

    if error.status == sys::BLE_HS_EDONE as u16 {
      log::debug!("Descriptor discovery complete");
      peer.discover_descriptors(conn_handle);
      return 0;
    }

    if error.status == 0 {
      let descriptor = ble::Descriptor::new(unsafe { &*dsc });
      log::debug!(
        "Discovered descriptor: {:?} ({})",
        descriptor.uuid,
        descriptor.handle
      );
      if let Some(service) = peer
        .services
        .iter_mut()
        .find(|s| s.start_handle == peer.current_service_start_handle)
      {
        if let Some(characteristic) = service
          .characteristics
          .iter_mut()
          .find(|c| c.value_handle == chr_val_handle)
        {
          characteristic.descriptors.push(descriptor);
        }
      }
      return 0;
    }

    peer.fail(error.status.into());
    0
  }
}
