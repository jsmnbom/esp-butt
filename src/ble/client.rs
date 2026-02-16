use std::{
  cell::RefCell,
  ffi::c_void,
  marker::PhantomPinned,
  pin::{Pin, pin},
  sync::atomic::AtomicU16,
};

use esp_idf_svc::sys::{self, BLE_HCI_CONN_ITVL, esp};
use smallvec::SmallVec;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::{
  ble::{self, utils::as_void_ptr},
  utils::ptr::voidp_to_ref,
};

#[derive(Debug, Clone)]
pub struct ConnectionParameters {
  pub scan_interval: core::time::Duration,
  pub scan_window: core::time::Duration,
  pub interval_min: core::time::Duration,
  pub interval_max: core::time::Duration,
  pub latency: u16,
  pub supervision_timeout: core::time::Duration,
  pub event_len_min: core::time::Duration,
  pub event_len_max: core::time::Duration,
}

impl From<ConnectionParameters> for sys::ble_gap_conn_params {
  fn from(value: ConnectionParameters) -> Self {
    Self {
      // Convert to 0.625ms units
      scan_itvl: (value.scan_interval.as_micros() as u32 / sys::BLE_HCI_SCAN_ITVL) as u16,
      scan_window: (value.scan_window.as_micros() as u32 / sys::BLE_HCI_SCAN_ITVL) as u16,
      // Convert to 1.25ms units
      itvl_min: (value.interval_min.as_micros() as u32 / sys::BLE_HCI_CONN_ITVL) as u16,
      itvl_max: (value.interval_max.as_micros() as u32 / sys::BLE_HCI_CONN_ITVL) as u16,
      // Convert to number of connection events
      latency: value.latency,
      // Convert to 10ms units
      supervision_timeout: (value.supervision_timeout.as_micros() / 10000) as u16,
      // Convert to 0.625ms units
      min_ce_len: (value.event_len_min.as_micros() / 625) as u16,
      max_ce_len: (value.event_len_max.as_micros() / 625) as u16,
    }
  }
}

impl From<sys::ble_gap_conn_params> for ConnectionParameters {
  fn from(value: sys::ble_gap_conn_params) -> Self {
    Self {
      scan_interval: core::time::Duration::from_micros(
        (value.scan_itvl as u64) * sys::BLE_HCI_SCAN_ITVL as u64,
      ),
      scan_window: core::time::Duration::from_micros(
        (value.scan_window as u64) * sys::BLE_HCI_SCAN_ITVL as u64,
      ),
      interval_min: core::time::Duration::from_micros(
        (value.itvl_min as u64) * sys::BLE_HCI_CONN_ITVL as u64,
      ),
      interval_max: core::time::Duration::from_micros(
        (value.itvl_max as u64) * sys::BLE_HCI_CONN_ITVL as u64,
      ),
      latency: value.latency,
      supervision_timeout: core::time::Duration::from_micros(
        (value.supervision_timeout as u64) * 10000,
      ),
      event_len_min: core::time::Duration::from_micros((value.min_ce_len as u64) * 625),
      event_len_max: core::time::Duration::from_micros((value.max_ce_len as u64) * 625),
    }
  }
}

impl Default for ConnectionParameters {
  fn default() -> Self {
    sys::ble_gap_conn_params {
      scan_itvl: sys::BLE_HCI_SCAN_ITVL_DEF as u16,
      scan_window: sys::BLE_HCI_SCAN_WINDOW_DEF as u16,
      itvl_min: 24, // 30ms
      itvl_max: 40, // 50ms
      latency: sys::BLE_GAP_INITIAL_CONN_LATENCY as u16,
      supervision_timeout: sys::BLE_GAP_INITIAL_SUPERVISION_TIMEOUT as u16,
      min_ce_len: sys::BLE_GAP_INITIAL_CONN_MIN_CE_LEN as u16,
      max_ce_len: sys::BLE_GAP_INITIAL_CONN_MAX_CE_LEN as u16,
    }
    .into()
  }
}

pub struct ClientConnector {
  address: ble::Address,
  conn_params: ConnectionParameters,
  connect_timeout: core::time::Duration,
}

impl ClientConnector {
  pub fn new(address: ble::Address) -> Self {
    Self {
      address,
      conn_params: ConnectionParameters::default(),
      connect_timeout: core::time::Duration::from_secs(30),
    }
  }

  pub fn connection_parameters(mut self, params: ConnectionParameters) -> Self {
    self.conn_params = params;
    self
  }

  pub fn connect_timeout(mut self, timeout: core::time::Duration) -> Self {
    self.connect_timeout = timeout;
    self
  }

  pub async fn connect(self) -> Result<Pin<Box<Client>>, ble::BleError> {
    unsafe {
      if sys::ble_gap_conn_find_by_addr(&self.address.into(), core::ptr::null_mut()) == 0 {
        ::log::warn!("A connection to {:?} already exists", self.address);
        return Err(ble::BleError::ConnectionFailed);
      }
    }

    let own_addr_type = ble::utils::get_own_address_type()?;
    let (tx, rx) = tokio::sync::oneshot::channel();

    let mut client = Box::into_pin(Box::new(Client::new(self.address, tx)));

    let client_ptr = Pin::as_ref(&client).get_ref() as *const Client as *mut c_void;

    unsafe {
      esp!(sys::ble_gap_connect(
        own_addr_type,
        &self.address.into(),
        self.connect_timeout.as_millis() as i32,
        &self.conn_params.into(),
        Some(Client::handle_gap_event),
        client_ptr,
      ))?;
    }

    match rx.await {
      Ok(Ok(conn_handle)) => {
        let client_ref = Pin::as_mut(&mut client);
        let client_mut = unsafe { client_ref.get_unchecked_mut() };
        client_mut.conn_handle = conn_handle;
        Ok(client)
      }
      Ok(Err(e)) => Err(e),
      Err(_) => Err(ble::BleError::ConnectionFailed),
    }
  }
}

#[derive(Debug, Default)]
pub struct Client {
  address: ble::Address,
  conn_handle: u16,
  conn_notifier: Option<oneshot::Sender<Result<u16, ble::BleError>>>,
  _pin: PhantomPinned,
}

impl Client {
  fn new(
    address: ble::Address,
    conn_notifier: oneshot::Sender<Result<u16, ble::BleError>>,
  ) -> Self {
    Self {
      address,
      conn_notifier: Some(conn_notifier),
      ..Default::default()
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
    let client = unsafe {
      let client_ptr = arg as *mut Client;
      // Safety: This assumes the pointer is valid and points to a `Client`.
      &mut *client_ptr
    };

    match event {
      ble::GapEvent::Connected {
        status,
        conn_handle,
      } => {
        if status == 0 {
          log::info!("Connected with handle {}", conn_handle);
          client.conn_notifier.take().map(|notifier| {
            let _ = notifier.send(Ok(conn_handle));
          });
        } else {
          log::error!("Failed to connect: status {}", status);
          client.conn_notifier.take().map(|notifier| {
            let _ = notifier.send(Err(ble::BleError::ConnectionFailed));
          });
        }
      }
      ble::GapEvent::Disconnected {
        reason,
        conn_handle,
      } => {
        log::info!(
          "Disconnected from handle {}: reason {}",
          conn_handle,
          reason
        );
      }
      _ => {
        log::info!("Received unhandled GAP event for client: {:?}", event);
      }
    }
    0
  }
}
