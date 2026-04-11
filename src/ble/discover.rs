use std::ffi::c_void;

use esp_idf_svc::sys::{self, esp_nofail};
use log::{info, warn};

use crate::ble::{AdReport, GapEvent};

pub trait DiscoveryListener {
  fn on_report(&mut self, report: &AdReport);
  fn on_complete(&mut self);
}

pub struct Discovery<'a, L: DiscoveryListener> {
  duration: Option<core::time::Duration>,
  limited: bool,
  passive: bool,
  filter_duplicates: bool,
  interval_625us: u16,
  window_625us: u16,
  listener: &'a mut L,
}

impl<'a, L: DiscoveryListener> Discovery<'a, L> {
  pub fn new(listener: &'a mut L) -> Self {
    Self {
      duration: None,
      limited: false,
      passive: false,
      filter_duplicates: true,
      interval_625us: 0,
      window_625us: 0,
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
    self.limited = limited;
    self
  }

  pub fn active(mut self, active: bool) -> Self {
    self.passive = !active;
    self
  }

  pub fn filter_duplicates(mut self, val: bool) -> Self {
    self.filter_duplicates = val;
    self
  }

  pub fn interval(mut self, interval_msecs: u16) -> Self {
    self.interval_625us = ((interval_msecs as f32) / 0.625) as u16;
    self
  }

  pub fn window(mut self, window_msecs: u16) -> Self {
    self.window_625us = ((window_msecs as f32) / 0.625) as u16;
    self
  }

  pub fn start(self) {
    let interval = self.interval_625us;
    let window = self.window_625us;
    let passive = self.passive;
    let filter_duplicates = self.filter_duplicates;
    let limited = self.limited;
    let duration_10ms = self
      .duration
      .map(|d| (d.as_millis() / 10).min(u16::MAX as u128) as u16)
      .unwrap_or(0);

    let self_ptr = Box::into_raw(Box::new(self));
    unsafe {
      let mut own_addr_type: u8 = 0;
      esp_nofail!(sys::ble_hs_id_infer_auto(0, &mut own_addr_type));

      let mut uncoded_params: sys::ble_gap_ext_disc_params = core::mem::zeroed();
      uncoded_params.itvl = interval;
      uncoded_params.window = window;
      uncoded_params.set_passive(passive as u8);

      let mut coded_params: sys::ble_gap_ext_disc_params = core::mem::zeroed();
      coded_params.itvl = interval;
      coded_params.window = window;
      coded_params.set_passive(passive as u8);

      esp_nofail!(sys::ble_gap_ext_disc(
        own_addr_type,
        duration_10ms,
        0, // no periodic scanning
        filter_duplicates as u8,
        sys::BLE_HCI_SCAN_FILT_NO_WL as u8,
        limited as u8,
        &uncoded_params,
        &coded_params,
        Some(Self::handle_gap_event),
        self_ptr as *mut c_void,
      ));
    }
  }

  pub fn stop() {
    // Safety: ... will leak memory if DiscoveryComplete is not called
    unsafe {
      sys::ble_gap_disc_cancel();
    }
  }

  extern "C" fn handle_gap_event(event: *mut sys::ble_gap_event, arg: *mut c_void) -> i32 {
    let event = match GapEvent::try_from(event) {
      Ok(e) => e,
      Err(e) => {
        ::log::error!("Failed to parse GAP event: {:?}", e);
        return 0;
      }
    };
    if arg.is_null() {
      warn!("Received null arg pointer in handle_gap_event");
      return -1;
    }

    let discovery = unsafe { &mut *arg.cast::<Self>() };

    match event {
      GapEvent::Discovery(ad_report) => discovery.listener.on_report(&ad_report),
      GapEvent::DiscoveryComplete { .. } => {
        discovery.listener.on_complete();
        unsafe { drop(Box::from_raw(arg as *mut Self)) }
      }
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
