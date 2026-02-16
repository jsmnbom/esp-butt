use bt_hci::FromHciBytesError;
use esp_idf_svc::sys;

mod address;
mod advertisement;
mod client;
mod gap;
mod init;
mod peripheral;
mod discover;
pub mod utils;

pub use address::*;
pub use advertisement::*;
pub use client::*;
pub use gap::*;
pub use init::init;
pub use peripheral::*;
pub use discover::*;

pub use bt_hci::param::{AddrKind, BdAddr};

#[derive(Debug)]
pub enum BleError {
  InvalidSize,
  InvalidValue,
  ConnectionFailed,
  Unimplemented,
  EspError(sys::EspError),
}

impl From<FromHciBytesError> for BleError {
  fn from(value: FromHciBytesError) -> Self {
    match value {
      FromHciBytesError::InvalidSize => Self::InvalidSize,
      FromHciBytesError::InvalidValue => Self::InvalidValue,
    }
  }
}

impl From<sys::EspError> for BleError {
  fn from(value: sys::EspError) -> Self {
    Self::EspError(value)
  }
}
