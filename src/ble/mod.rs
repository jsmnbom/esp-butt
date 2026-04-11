use esp_idf_svc::sys;

mod address;
mod advertisement;
mod client;
mod discover;
mod gap;
mod gatt;
mod init;
mod peer;
mod peripheral;
pub mod utils;

pub use address::*;
pub use advertisement::*;
pub use client::*;
pub use discover::*;
pub use gap::*;
pub use gatt::*;
pub use init::init;
pub use peer::*;
pub use peripheral::*;

#[derive(Debug, Clone)]
pub enum BleError {
  InvalidSize,
  InvalidValue,
  ConnectionFailed,
  Unimplemented,
  MissingNotifyDescriptor,
  Internal,
  Timeout,
  EspError(sys::EspError),
  NimbleError(u16),
}

impl From<sys::EspError> for BleError {
  fn from(value: sys::EspError) -> Self {
    Self::EspError(value)
  }
}

impl From<u16> for BleError {
  fn from(value: u16) -> Self {
    Self::NimbleError(value)
  }
}
