use std::ffi::c_void;
use std::marker::PhantomPinned;
use std::pin::Pin;

use esp_idf_svc::sys::{self, esp};
use tokio::sync::oneshot;

use crate::ble::{BleError, Characteristic, Descriptor, Service};

#[derive(Debug)]
pub struct Peer {
  pub services: Vec<Service>,

  tx: Option<oneshot::Sender<Result<(), BleError>>>,

  _pin: PhantomPinned,
}

impl Peer {
  pub async fn discover_services(conn_handle: u16) -> Result<Vec<Service>, BleError> {
    let (tx, rx) = oneshot::channel();
    let mut peer = Box::pin(Self {
      services: Vec::new(),
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
      Err(_) => Err(BleError::ConnectionFailed),
    }
  }

  fn as_raw(&self) -> *mut c_void {
    self as *const Self as *mut c_void
  }

  fn into_services(self) -> Vec<Service> {
    self.services
  }

  fn fail(&mut self, error: BleError) {
    log::error!("Peer discovery failed: {:?}", error);
    let _ = self.tx.take().unwrap().send(Err(error));
  }

  fn complete(&mut self) {
    log::debug!("Peer discovery complete");
    let _ = self.tx.take().unwrap().send(Ok(()));
  }

  fn start_discover_services(&mut self, conn_handle: u16) {
    log::debug!("Discovering services");
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
    let start_handle = self.services.iter().map(|s| s.start_handle).min();
    let end_handle = self.services.iter().map(|s| s.end_handle).max();

    let (Some(start_handle), Some(end_handle)) = (start_handle, end_handle) else {
      self.discover_all_descriptors(conn_handle);
      return;
    };

    log::trace!("Discovering all characteristics (handles {}-{})", start_handle, end_handle);

    unsafe {
      if let Err(error) = esp!(sys::ble_gattc_disc_all_chrs(
        conn_handle,
        start_handle,
        end_handle,
        Some(Self::on_gatt_chr),
        self.as_raw(),
      )) {
        self.fail(error.into());
      }
    }
  }

  fn discover_all_descriptors(&mut self, conn_handle: u16) {
    // Compute the tightest range that spans all known characteristic value
    // handles up to the last service end handle.  Using the first
    // characteristic's value handle (not a service start handle) as the lower
    // bound avoids including service/characteristic declaration attributes that
    // can never be descriptors.
    let start_handle = self
      .services
      .iter()
      .flat_map(|s| s.characteristics.first())
      .map(|c| c.value_handle)
      .min();
    let end_handle = self.services.iter().map(|s| s.end_handle).max();

    let (Some(start_handle), Some(end_handle)) = (start_handle, end_handle) else {
      // No characteristics at all; nothing to discover.
      self.complete();
      return;
    };

    if start_handle >= end_handle {
      self.complete();
      return;
    }

    log::trace!("Discovering all descriptors (handles {}-{})", start_handle, end_handle);

    unsafe {
      if let Err(error) = esp!(sys::ble_gattc_disc_all_dscs(
        conn_handle,
        start_handle,
        end_handle,
        Some(Self::on_gatt_dsc),
        self.as_raw(),
      )) {
        self.fail(error.into());
      }
    }
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
      log::trace!(
        "Service discovery finished, found {} services",
        peer.services.len()
      );
      // All services discovered, start discovery of characteristics
      peer.discover_characteristics(conn_handle);
      return 0;
    }

    if error.status == 0 {
      let service = unsafe { &(*service) };
      let svc = Service::new(service);
      log::trace!(
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
      peer.discover_all_descriptors(conn_handle);
      return 0;
    }

    if error.status == 0 {
      let chr = unsafe { &(*chr) };
      let characteristic = Characteristic::new(conn_handle, chr);
      // Attribute the characteristic to whichever service owns its handle range.
      for service in &mut peer.services {
        if characteristic.definition_handle >= service.start_handle
          && characteristic.definition_handle <= service.end_handle
        {
          log::trace!(
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
    _conn_handle: u16,
    error: *const sys::ble_gatt_error,
    _chr_val_handle: u16,
    dsc: *const sys::ble_gatt_dsc,
    arg: *mut c_void,
  ) -> i32 {
    let peer: &mut Peer = unsafe { &mut *(arg as *mut Peer) };
    let error = unsafe { &*error };

    if error.status == sys::BLE_HS_EDONE as u16 {
      log::trace!("Descriptor discovery complete");
      peer.complete();
      return 0;
    }

    if error.status == 0 {
      let descriptor = Descriptor::new(unsafe { &*dsc });
      let dsc_handle = descriptor.handle;
      // disc_all_dscs returns every attribute in the range, including
      // characteristic declarations (0x2803) and characteristic value
      // attributes. Find the owning service and characteristic by handle
      // range, and skip anything that isn't an actual descriptor.
      'outer: for service in peer.services.iter_mut() {
        let num_chars = service.characteristics.len();
        for i in 0..num_chars {
          let chr = &service.characteristics[i];
          // Handles at or before the value handle belong to the declaration
          // or value attribute, not a descriptor.
          if dsc_handle <= chr.value_handle {
            break 'outer;
          }
          let chr_end = if i + 1 < num_chars {
            service.characteristics[i + 1].definition_handle - 1
          } else {
            service.end_handle
          };
          if dsc_handle <= chr_end {
            log::trace!(
              "Discovered descriptor: {:?} ({})",
              descriptor.uuid,
              dsc_handle
            );
            service.characteristics[i].descriptors.push(descriptor);
            break 'outer;
          }
        }
      }
      return 0;
    }

    peer.fail(error.status.into());
    0
  }
}
