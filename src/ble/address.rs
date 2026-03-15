use esp_idf_svc::sys;
use strum::{Display, FromRepr};

use crate::ble::BleError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, FromRepr, Display)]
#[repr(u8)]
pub enum AddrKind {
  Public = sys::BLE_ADDR_PUBLIC as _,
  Random = sys::BLE_ADDR_RANDOM as _,
  PublicId = sys::BLE_ADDR_RANDOM_ID as _,
  RandomId = sys::BLE_ADDR_PUBLIC_ID as _,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BdAddr([u8; 6]);

impl BdAddr {
  pub fn new(addr: [u8; 6]) -> Self {
    Self(addr)
  }
}

impl From<[u8; 6]> for BdAddr {
  fn from(value: [u8; 6]) -> Self {
    Self(value)
  }
}

impl From<&[u8; 6]> for BdAddr {
  fn from(value: &[u8; 6]) -> Self {
    Self(*value)
  }
}

impl From<BdAddr> for [u8; 6] {
  fn from(value: BdAddr) -> Self {
    value.0
  }
}

impl From<&BdAddr> for [u8; 6] {
  fn from(value: &BdAddr) -> Self {
    value.0
  }
}

impl core::fmt::Display for BdAddr {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(
      fmt,
      "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
      self.0[5], self.0[4], self.0[3], self.0[2], self.0[1], self.0[0]
    )
  }
}

impl core::fmt::Debug for BdAddr {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(fmt, "{}", self)
  }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
  pub kind: AddrKind,
  pub addr: BdAddr,
}

impl TryFrom<sys::ble_addr_t> for Address {
  type Error = BleError;

  fn try_from(value: sys::ble_addr_t) -> Result<Self, Self::Error> {
    let kind = AddrKind::from_repr(value.type_ as _).ok_or(BleError::InvalidValue)?;
    let addr = BdAddr(value.val);
    Ok(Self { kind, addr })
  }
}

impl core::fmt::Display for Address {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(fmt, "{}", self.addr)
  }
}

impl core::fmt::Debug for Address {
  fn fmt(&self, fmt: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(fmt, "{} ({})", self.addr, self.kind)
  }
}

impl From<Address> for sys::ble_addr_t {
  fn from(value: Address) -> Self {
    Self {
      type_: value.kind as _,
      val: value.addr.0,
    }
  }
}

impl From<&Address> for sys::ble_addr_t {
  fn from(value: &Address) -> Self {
    Self {
      type_: value.kind as _,
      val: value.addr.0,
    }
  }
}
