use esp_idf_svc::sys;
use log::warn;

use crate::ble;

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
  Discovery(ble::AdReport<'a>),
  DiscoveryComplete {
    reason: i32,
  },
  ConnectionUpdate {
    status: i32,
    conn_handle: u16,
  },
  TerminationFailed {},
  NotifyRx {},
  NotifyTx {},
  Subscribe {},
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
}

impl<'a> TryFrom<&'a sys::ble_gap_event> for GapEvent<'a> {
  type Error = ble::BleError;

  fn try_from(value: &'a sys::ble_gap_event) -> Result<Self, Self::Error> {
    match value.type_ as u32 {
      sys::BLE_GAP_EVENT_CONNECT => {
        let connect = unsafe { &value.__bindgen_anon_1.connect };
        Ok(Self::Connected {
          status: connect.status,
          conn_handle: connect.conn_handle,
        })
      },
      sys::BLE_GAP_EVENT_LINK_ESTAB => {
        let link_established = unsafe { &value.__bindgen_anon_1.link_estab };
        Ok(Self::LinkEstablished {
          status: link_established.status,
          conn_handle: link_established.conn_handle,
        })
      },
      sys::BLE_GAP_EVENT_DISCONNECT => {
        let disconnect = unsafe { &value.__bindgen_anon_1.disconnect };
        Ok(Self::Disconnected {
          reason: disconnect.reason,
          conn_handle: disconnect.conn.conn_handle
        })
      }
      sys::BLE_GAP_EVENT_L2CAP_UPDATE_REQ => {
        Ok(Self::L2CapUpdateRequest {})
      }
      sys::BLE_GAP_EVENT_DISC => {
        let disc = unsafe { &value.__bindgen_anon_1.disc };
        Ok(Self::Discovery(ble::AdReport::try_from(disc)?))
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
      sys::BLE_GAP_EVENT_PHY_UPDATE_COMPLETE => {
        let phy_update = unsafe { &value.__bindgen_anon_1.phy_updated };
        Ok(Self::PhyUpdated {
          status: phy_update.status,
          conn_handle: phy_update.conn_handle,
          tx_phy: phy_update.tx_phy,
          rx_phy: phy_update.rx_phy,
        })
      }
     
      _ => {
        warn!("Received unhandled GAP event type: {}", value.type_);
        Err(ble::BleError::Unimplemented)
      }
     
    }
  }
}
