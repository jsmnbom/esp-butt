use std::{
  ffi::c_void,
  sync::atomic::{AtomicBool, AtomicU16, Ordering},
};

use esp_idf_svc::sys::{self, esp, esp_nofail};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::ble::{Address, BleError, GapEvent, Peer, Service, utils};

#[derive(Debug, Clone)]
pub struct Notification {
  pub attr_handle: u16,
  pub data: Vec<u8>,
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
    Self {
      scan_interval: core::time::Duration::from_millis(100),
      scan_window: core::time::Duration::from_millis(50),
      interval_min: core::time::Duration::from_millis(15),
      interval_max: core::time::Duration::from_millis(30),
      latency: 0,
      supervision_timeout: core::time::Duration::from_millis(2500),
      event_len_min: core::time::Duration::from_millis(5),
      event_len_max: core::time::Duration::from_millis(100),
    }
  }
}

pub struct ClientConnector {
  address: Address,
  conn_params: ConnectionParameters,
  connect_timeout: core::time::Duration,
}

impl ClientConnector {
  pub fn new(address: Address) -> Self {
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

  pub async fn connect(self) -> Result<Client, BleError> {
    unsafe {
      if sys::ble_gap_conn_find_by_addr(&self.address.into(), core::ptr::null_mut()) == 0 {
        ::log::warn!("A connection to {:?} already exists", self.address);
        return Err(BleError::ConnectionFailed);
      }
    }

    let own_addr_type = utils::get_own_address_type()?;
    let (tx, mut rx) = broadcast::channel(64);

    let state = Box::new(ClientState {
      connected: AtomicBool::new(false),
      conn_handle: AtomicU16::new(0),
      tx,
    });
    let state_ptr = state.as_ref() as *const _ as *mut c_void;

    log::info!("Initiating connection to {:?} with params: {:?}", self.address, self.conn_params);

    let raw_params: sys::ble_gap_conn_params = self.conn_params.into();
    unsafe {
      esp!(sys::ble_gap_ext_connect(
        own_addr_type,
        &self.address.into(),
        self.connect_timeout.as_millis() as i32,
        (sys::BLE_GAP_LE_PHY_1M_MASK | sys::BLE_GAP_LE_PHY_CODED_MASK) as u8,
        &raw_params,
        &raw_params,
        &raw_params,
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
              Peer::discover_services(state.conn_handle.load(Ordering::SeqCst)).await?;

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
        Err(_) => return Err(BleError::ConnectionFailed),
      }
    }
  }
}

#[derive(Debug)]
pub struct ClientState {
  connected: AtomicBool,
  conn_handle: AtomicU16,
  tx: broadcast::Sender<Result<ClientEvent, BleError>>,
}

#[derive(Debug)]
pub struct Client {
  state: Box<ClientState>,
  address: Address,
  services: Vec<Service>,
}

impl Client {
  pub fn address(&self) -> Address {
    self.address
  }

  pub fn events(&self) -> broadcast::Receiver<Result<ClientEvent, BleError>> {
    self.state.tx.subscribe()
  }

  #[allow(dead_code)]
  pub fn services_iter(&self) -> impl Iterator<Item = &Service> {
    self.services.iter()
  }

  #[allow(dead_code)]
  pub fn get_service(&self, uuid: Uuid) -> Option<&Service> {
    self.services.iter().find(|s| s.uuid == uuid)
  }

  pub fn conn_handle(&self) -> u16 {
    self.state.conn_handle.load(Ordering::SeqCst)
  }

  pub fn disconnect(&self) -> Result<(), BleError> {
    if !self.state.connected.load(Ordering::SeqCst) {
      return Ok(());
    }

    let conn_handle = self.state.conn_handle.load(Ordering::SeqCst);
    let rc = unsafe {
      sys::ble_gap_terminate(
        conn_handle,
        sys::ble_error_codes_BLE_ERR_REM_USER_CONN_TERM as u8,
      )
    };

    if rc != 0 && rc != sys::BLE_HS_ENOTCONN as i32 {
      return Err(BleError::NimbleError(rc as u16));
    }

    Ok(())
  }
}

pub fn read_rssi_for_conn(conn_handle: u16) -> Result<i8, BleError> {
  let mut rssi: i8 = 0;
  esp!(unsafe { sys::ble_gap_conn_rssi(conn_handle, &mut rssi) })?;
  Ok(rssi)
}

extern "C" fn on_gap_event(event: *mut sys::ble_gap_event, arg: *mut c_void) -> i32 {
  let event = match GapEvent::try_from(unsafe { &*event }) {
    Ok(e) => e,
    Err(e) => {
      log::error!("Failed to parse GAP event: {:?}", e);
      return 0;
    }
  };
  let state: &ClientState = unsafe { &*arg.cast() };

  log::trace!("GAP event: {:?}", event);

  match event {
    GapEvent::Connected {
      status,
      conn_handle,
    } => {
      if status == 0 {
        log::info!("Connected with handle {}", conn_handle);
        state.connected.store(true, Ordering::SeqCst);
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
        let _ = state.tx.send(Err(BleError::ConnectionFailed));
      }
    }
    GapEvent::LinkEstablished { status, conn_handle } => {
      log::info!("Link established (handle {})", conn_handle);
      if status == 0 {
        let rc = unsafe {
          sys::ble_gap_set_prefered_le_phy(
            conn_handle,
            sys::BLE_GAP_LE_PHY_CODED_MASK as u8,
            sys::BLE_GAP_LE_PHY_CODED_MASK as u8,
            sys::BLE_GAP_LE_PHY_CODED_S8 as u16,
          )
        };
        if rc != 0 {
          log::warn!("Failed to request S=8 coding on conn {}: {}", conn_handle, rc);
        }
      }
    }
    GapEvent::Mtu { mtu, .. } => {
      log::info!("MTU exchanged: {}", mtu);
    }
    GapEvent::Disconnected {
      reason,
      conn_handle,
    } => {
      state.connected.store(false, Ordering::SeqCst);
      log::info!(
        "Disconnected from handle {}: reason {}",
        conn_handle,
        reason
      );
      let _ = state.tx.send(Ok(ClientEvent::Disconnected));
      // TODO: make sure inner state gets dropped
    }
    GapEvent::NotifyRx {
      attr_handle, om, ..
    } => {
      let data = utils::os_mbuf_to_vec(om);

      let notification = Notification { attr_handle, data };
      let _ = state.tx.send(Ok(ClientEvent::Notification(notification)));
    }
    GapEvent::PhyUpdated {
      status,
      conn_handle,
      tx_phy,
      rx_phy,
    } => {
      if status == 0 {
        log::info!(
          "PHY updated for conn {}: TX={} RX={}",
          conn_handle,
          utils::phy_name(tx_phy),
          utils::phy_name(rx_phy),
        );
      } else {
        log::warn!("PHY update failed for conn {}: status {}", conn_handle, status);
      }
    }
    _ => {
      log::info!("Received unhandled GAP event for client: {:?}", event);
    }
  }
  0
}
