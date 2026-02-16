use bt_hci::FromHciBytes;
use bt_hci::param::AddrKind;
use bt_hci::param::BdAddr;
use esp_idf_svc::sys;

use crate::ble::BleError;

#[derive(Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
  pub kind: AddrKind,
  pub addr: BdAddr,
}

impl TryFrom<sys::ble_addr_t> for Address {
  type Error = BleError;

  fn try_from(value: sys::ble_addr_t) -> Result<Self, Self::Error> {
    let (kind, _) = AddrKind::from_hci_bytes(&[value.type_]).map_err(BleError::from)?;
    let (addr, _) = BdAddr::from_hci_bytes(&value.val).map_err(BleError::from)?;
    Ok(Self { kind, addr })
  }
}

impl core::fmt::Debug for Address {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(
      fmt,
      "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
      self.addr.0[5],
      self.addr.0[4],
      self.addr.0[3],
      self.addr.0[2],
      self.addr.0[1],
      self.addr.0[0]
    )?;
    match self.kind {
      AddrKind::PUBLIC => write!(fmt, " (public)")?,
      AddrKind::RANDOM => write!(fmt, " (random)")?,
      AddrKind::RESOLVABLE_PRIVATE_OR_PUBLIC => write!(fmt, " (publicid)")?,
      AddrKind::RESOLVABLE_PRIVATE_OR_RANDOM => write!(fmt, " (randomid)")?,
      _ => {}
    };
    Ok(())
  }
}

impl From<Address> for sys::ble_addr_t {
  fn from(value: Address) -> Self {
    Self {
      type_: value.kind.as_raw() as _,
      val: value.addr.0,
    }
  }
}

impl From<&Address> for sys::ble_addr_t {
  fn from(value: &Address) -> Self {
    Self {
      type_: value.kind.as_raw() as _,
      val: value.addr.0,
    }
  }
}
