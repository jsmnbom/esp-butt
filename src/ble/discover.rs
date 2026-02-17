use std::ffi::c_void;

use esp_idf_svc::sys::{self, esp_nofail};
use log::{info, warn};

use crate::{ble, utils::ptr::voidp_to_ref};

pub trait DiscoveryListener {
  fn on_report(&mut self, report: &ble::AdReport);
  fn on_complete(&mut self);
}

pub struct Discovery<'a, L: DiscoveryListener> {
  duration: Option<core::time::Duration>,
  params: sys::ble_gap_disc_params,
  listener: &'a mut L,
}

impl<'a, L: DiscoveryListener> Discovery<'a, L> {
  pub fn new(listener: &'a mut L) -> Self {
    Self {
      duration: None,
      params: sys::ble_gap_disc_params {
        itvl: 0,
        window: 0,
        filter_policy: sys::BLE_HCI_SCAN_FILT_NO_WL as _,
        ..Default::default()
      },
      listener,
    }
    .limited(false)
    .filter_duplicates(true)
    .active(true)
    .window(30)
    .interval(30)
    .duration(core::time::Duration::from_secs(10))
  }

  pub fn duration(mut self, duration: core::time::Duration) -> Self {
    self.duration = Some(duration);
    self
  }

  pub fn limited(mut self, limited: bool) -> Self {
    self.params.set_limited(limited as _);
    self
  }

  pub fn active(mut self, active: bool) -> Self {
    self.params.set_passive((!active) as _);
    self
  }

  pub fn filter_duplicates(mut self, val: bool) -> Self {
    self.params.set_filter_duplicates(val as _);
    self
  }

  pub fn interval(mut self, interval_msecs: u16) -> Self {
    self.params.itvl = ((interval_msecs as f32) / 0.625) as u16;
    self
  }

  pub fn window(mut self, window_msecs: u16) -> Self {
    self.params.window = ((window_msecs as f32) / 0.625) as u16;
    self
  }

  pub fn start(self) {
    let self_ptr = Box::into_raw(Box::new(self));
    unsafe {
      let mut own_addr_type: u8 = 0;
      esp_nofail!(sys::ble_hs_id_infer_auto(0, &mut own_addr_type));
      sys::ble_gap_disc(
        own_addr_type,
        (*self_ptr)
          .duration
          .map(|d| d.as_millis() as i32)
          .unwrap_or(0),
        &(*self_ptr).params,
        Some(Self::handle_gap_event),
        self_ptr as *mut c_void,
      );
    }
  }

  pub fn stop() {
    unsafe {
      sys::ble_gap_disc_cancel();
    }
  }

  extern "C" fn handle_gap_event(event: *mut sys::ble_gap_event, arg: *mut c_void) -> i32 {
    let event = match ble::GapEvent::try_from(unsafe { &*event }) {
      Ok(e) => e,
      Err(e) => {
        ::log::error!("Failed to parse GAP event: {:?}", e);
        return 0;
      }
    };
    let discovery = unsafe { voidp_to_ref::<Self>(arg) };

    if arg.is_null() {
      warn!("Received null arg pointer in handle_gap_event");
      return -1;
    }

    match event {
      ble::GapEvent::Discovery(ad_report) => discovery.listener.on_report(&ad_report),
      ble::GapEvent::DiscoveryComplete { .. } => discovery.listener.on_complete(),
      _ => {
        info!(
          "Received unhandled GAP event while discovering: {:?}",
          event
        );
      }
    }
    0
  }
}
