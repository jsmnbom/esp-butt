use std::{
  collections::HashMap,
  ffi::c_void,
  sync::{
    Mutex,
    atomic::{AtomicU16, Ordering},
  },
};

use allocator_api2::vec::Vec;
use esp_idf_svc::sys::{self, esp, esp_nofail};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::{
  ble,
  utils::{heap::ExternalMemory, os_mbuf},
};

#[derive(Debug, Clone)]
pub struct Notification {
  pub attr_handle: u16,
  pub data: std::vec::Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum ClientEvent {
  Connected,
  Disconnected,
  Notification(Notification),
}

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

  #[allow(dead_code)]
  pub fn connection_parameters(mut self, params: ConnectionParameters) -> Self {
    self.conn_params = params;
    self
  }

  #[allow(dead_code)]
  pub fn connect_timeout(mut self, timeout: core::time::Duration) -> Self {
    self.connect_timeout = timeout;
    self
  }

  pub async fn connect(self) -> Result<Client, ble::BleError> {
    unsafe {
      if sys::ble_gap_conn_find_by_addr(&self.address.into(), core::ptr::null_mut()) == 0 {
        ::log::warn!("A connection to {:?} already exists", self.address);
        return Err(ble::BleError::ConnectionFailed);
      }
    }

    let own_addr_type = ble::utils::get_own_address_type()?;
    let (tx, mut rx) = broadcast::channel(16);

    let state = Box::new(ClientState {
      conn_handle: AtomicU16::new(0),
      tx,
      handle_to_uuid: Mutex::new(HashMap::new()),
    });
    let state_ptr = state.as_ref() as *const _ as *mut c_void;

    unsafe {
      esp!(sys::ble_gap_connect(
        own_addr_type,
        &self.address.into(),
        self.connect_timeout.as_millis() as i32,
        &self.conn_params.into(),
        Some(on_gap_event),
        state_ptr,
      ))?;
    }

    loop {
      let event = rx.recv().await;
      match event {
        Ok(Ok(event)) => match event {
          ClientEvent::Connected => {
            let services =
              ble::Peer::discover_services(state.conn_handle.load(Ordering::SeqCst)).await?;

            {
              let mut map = state.handle_to_uuid.lock().unwrap();
              for service in &services {
                for characteristic in &service.characteristics {
                  map.insert(characteristic.value_handle, characteristic.uuid);
                }
              }
            }

            let client = Client {
              state,
              address: self.address,
              services,
            };
            return Ok(client);
          }
          _ => continue,
        },
        Ok(Err(e)) => return Err(e),
        Err(_) => return Err(ble::BleError::ConnectionFailed),
      }
    }
  }
}

#[derive(Debug)]
pub struct ClientState {
  conn_handle: AtomicU16,
  tx: broadcast::Sender<Result<ClientEvent, ble::BleError>>,
  handle_to_uuid: Mutex<HashMap<u16, Uuid>>,
}

#[derive(Debug)]
pub struct Client {
  state: Box<ClientState>,
  address: ble::Address,
  services: Vec<ble::Service, ExternalMemory>,
}

impl Client {
  pub fn address(&self) -> ble::Address {
    self.address
  }

  pub fn events(&self) -> broadcast::Receiver<Result<ClientEvent, ble::BleError>> {
    self.state.tx.subscribe()
  }

  #[allow(dead_code)]
  pub fn services_iter(&self) -> impl Iterator<Item = &ble::Service> {
    self.services.iter()
  }

  #[allow(dead_code)]
  pub fn get_service(&self, uuid: Uuid) -> Option<&ble::Service> {
    self.services.iter().find(|s| s.uuid == uuid)
  }
}

extern "C" fn on_gap_event(event: *mut sys::ble_gap_event, arg: *mut c_void) -> i32 {
  let event = match ble::GapEvent::try_from(unsafe { &*event }) {
    Ok(e) => e,
    Err(e) => {
      ::log::error!("Failed to parse GAP event: {:?}", e);
      return 0;
    }
  };
  let state: &ClientState = unsafe { &*arg.cast() };

  match event {
    ble::GapEvent::Connected {
      status,
      conn_handle,
    } => {
      if status == 0 {
        log::info!("Connected with handle {}", conn_handle);
        state.conn_handle.store(conn_handle, Ordering::SeqCst);
        log::info!("Exchanging MTU");
        unsafe {
          esp_nofail!(sys::ble_gattc_exchange_mtu(
            conn_handle,
            None,
            core::ptr::null_mut()
          ))
        }
        let _ = state.tx.send(Ok(ClientEvent::Connected));
      } else {
        log::error!("Failed to connect: status {}", status);
        let _ = state.tx.send(Err(ble::BleError::ConnectionFailed));
      }
    }
    ble::GapEvent::LinkEstablished { .. } => {
      log::info!("Link established");
    }
    ble::GapEvent::Mtu { mtu, .. } => {
      log::info!("MTU exchanged: {}", mtu);
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
      let _ = state.tx.send(Ok(ClientEvent::Disconnected));
      // TODO: make sure inner state gets dropped
    }
    ble::GapEvent::NotifyRx {
      attr_handle, om, ..
    } => {
      let mbuf = os_mbuf::OsMBuf(om);
      let data_slice = mbuf.as_flat();
      // Unfortunately needs to be std::vec::Vec so we can pass it to buttplug
      let mut data = std::vec::Vec::with_capacity(data_slice.as_slice().len());
      data.extend_from_slice(data_slice.as_slice());

      let notification = Notification { attr_handle, data };
      let _ = state.tx.send(Ok(ClientEvent::Notification(notification)));
    }
    _ => {
      log::info!("Received unhandled GAP event for client: {:?}", event);
    }
  }
  0
}
