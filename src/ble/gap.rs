use esp_idf_svc::sys;
use log::warn;

use crate::ble::{AdReport, BleError};

#[allow(dead_code)]
#[derive(Debug)]
pub enum GapEvent<'a> {
  Connected {
    status: i32,
    conn_handle: u16,
  },
  LinkEstablished {
    status: i32,
    conn_handle: u16,
  },
  Disconnected {
    reason: i32,
    conn_handle: u16,
  },
  L2CapUpdateRequest {},
  Discovery(AdReport<'a>),
  DiscoveryComplete {
    reason: i32,
  },
  ConnectionUpdate {
    status: i32,
    conn_handle: u16,
  },
  TerminationFailed {},
  NotifyRx {
    conn_handle: u16,
    attr_handle: u16,
    om: *mut sys::os_mbuf,
    indication: bool,
  },
  NotifyTx {
    status: i32,
    conn_handle: u16,
    attr_handle: u16,
    indication: bool,
  },
  Subscribe {
    conn_handle: u16,
    attr_handle: u16,
    reason: u16,
    prev_notify: bool,
    cur_notify: bool,
    prev_indicate: bool,
    cur_indicate: bool,
  },
  Mtu {
    conn_handle: u16,
    channel_id: u16,
    mtu: u16,
  },
  PhyUpdated {
    status: i32,
    conn_handle: u16,
    tx_phy: u8,
    rx_phy: u8,
  },
  DataLengthChanged {
    conn_handle: u16,
    max_tx_octets: u16,
    max_tx_time: u16,
    max_rx_octets: u16,
    max_rx_time: u16,
  },
}

impl TryFrom<*mut sys::ble_gap_event> for GapEvent<'static> {
  type Error = BleError;

  fn try_from(value: *mut sys::ble_gap_event) -> Result<Self, Self::Error> {
    if value.is_null() {
      warn!("Received null event pointer");
      return Err(BleError::Internal);
    }
    let event = unsafe { &*value };
    event.try_into()
  }
}

impl<'a> TryFrom<&'a sys::ble_gap_event> for GapEvent<'a> {
  type Error = BleError;

  fn try_from(value: &'a sys::ble_gap_event) -> Result<Self, Self::Error> {
    match value.type_ as u32 {
      sys::BLE_GAP_EVENT_CONNECT => {
        let connect = unsafe { &value.__bindgen_anon_1.connect };
        Ok(Self::Connected {
          status: connect.status,
          conn_handle: connect.conn_handle,
        })
      }
      sys::BLE_GAP_EVENT_LINK_ESTAB => {
        let link_established = unsafe { &value.__bindgen_anon_1.link_estab };
        Ok(Self::LinkEstablished {
          status: link_established.status,
          conn_handle: link_established.conn_handle,
        })
      }
      sys::BLE_GAP_EVENT_DISCONNECT => {
        let disconnect = unsafe { &value.__bindgen_anon_1.disconnect };
        Ok(Self::Disconnected {
          reason: disconnect.reason,
          conn_handle: disconnect.conn.conn_handle,
        })
      }
      sys::BLE_GAP_EVENT_L2CAP_UPDATE_REQ => Ok(Self::L2CapUpdateRequest {}),
      sys::BLE_GAP_EVENT_DISC => {
        let disc = unsafe { &value.__bindgen_anon_1.disc };
        Ok(Self::Discovery(AdReport::try_from(disc)?))
      }
      sys::BLE_GAP_EVENT_EXT_DISC => {
        let ext_disc = unsafe { &value.__bindgen_anon_1.ext_disc };
        Ok(Self::Discovery(AdReport::try_from(ext_disc)?))
      }
      sys::BLE_GAP_EVENT_DISC_COMPLETE => {
        let disc_complete = unsafe { &value.__bindgen_anon_1.disc_complete };
        Ok(Self::DiscoveryComplete {
          reason: disc_complete.reason,
        })
      }
      sys::BLE_GAP_EVENT_CONN_UPDATE => {
        let conn_update = unsafe { &value.__bindgen_anon_1.conn_update };
        Ok(Self::ConnectionUpdate {
          status: conn_update.status,
          conn_handle: conn_update.conn_handle,
        })
      }
      sys::BLE_GAP_EVENT_MTU => {
        let mtu = unsafe { &value.__bindgen_anon_1.mtu };
        Ok(Self::Mtu {
          conn_handle: mtu.conn_handle,
          channel_id: mtu.channel_id,
          mtu: mtu.value,
        })
      }
      sys::BLE_GAP_EVENT_NOTIFY_RX => {
        let notify_rx = unsafe { &value.__bindgen_anon_1.notify_rx };
        Ok(Self::NotifyRx {
          conn_handle: notify_rx.conn_handle,
          attr_handle: notify_rx.attr_handle,
          om: notify_rx.om,
          indication: notify_rx.indication() != 0,
        })
      }
      sys::BLE_GAP_EVENT_NOTIFY_TX => {
        let notify_tx = unsafe { &value.__bindgen_anon_1.notify_tx };
        Ok(Self::NotifyTx {
          status: notify_tx.status,
          conn_handle: notify_tx.conn_handle,
          attr_handle: notify_tx.attr_handle,
          indication: notify_tx.indication() != 0,
        })
      }
      sys::BLE_GAP_EVENT_SUBSCRIBE => {
        let subscribe = unsafe { &value.__bindgen_anon_1.subscribe };
        Ok(Self::Subscribe {
          conn_handle: subscribe.conn_handle,
          attr_handle: subscribe.attr_handle,
          reason: subscribe.reason as u16,
          prev_notify: subscribe.prev_notify() != 0,
          cur_notify: subscribe.cur_notify() != 0,
          prev_indicate: subscribe.prev_indicate() != 0,
          cur_indicate: subscribe.cur_indicate() != 0,
        })
      }
      sys::BLE_GAP_EVENT_PHY_UPDATE_COMPLETE => {
        let phy_update = unsafe { &value.__bindgen_anon_1.phy_updated };
        Ok(Self::PhyUpdated {
          status: phy_update.status,
          conn_handle: phy_update.conn_handle,
          tx_phy: phy_update.tx_phy,
          rx_phy: phy_update.rx_phy,
        })
      }
      sys::BLE_GAP_EVENT_DATA_LEN_CHG => {
        let data_len_chg = unsafe { &value.__bindgen_anon_1.data_len_chg };
        Ok(Self::DataLengthChanged {
          conn_handle: data_len_chg.conn_handle,
          max_tx_octets: data_len_chg.max_tx_octets,
          max_tx_time: data_len_chg.max_tx_time,
          max_rx_octets: data_len_chg.max_rx_octets,
          max_rx_time: data_len_chg.max_rx_time,
        })
      }

      _ => {
        warn!("Received unhandled GAP event type: {}", value.type_);
        Err(BleError::Unimplemented)
      }
    }
  }
}
