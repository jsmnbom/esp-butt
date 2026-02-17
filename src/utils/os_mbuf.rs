use core::ffi::c_int;
use std::vec::Vec;

use esp_idf_svc::sys;
use sys::os_mbuf_append as _os_mbuf_append;

#[derive(Copy, Clone, Debug)]
pub(crate) struct OsMBuf(pub *mut sys::os_mbuf);

impl OsMBuf {
  #[inline]
  pub fn is_null(&self) -> bool {
    self.0.is_null()
  }

  #[inline]
  pub fn as_slice<'a>(&self) -> &'a [u8] {
    if self.is_null() {
      return &[];
    }
    unsafe { core::slice::from_raw_parts((*self.0).om_data, (*self.0).om_len as _) }
  }

  #[inline]
  pub fn len(&self) -> u16 {
    if self.is_null() {
      0
    } else {
      unsafe { (*self.0).om_len }
    }
  }

  #[inline]
  pub fn entire_len(&self) -> u16 {
    if self.is_null() {
      return 0;
    }
    unsafe {
      // OS_MBUF_F_HAS_PKTHDR is 0x01 in Mynewt
      if ((*self.0).om_flags & 0x01) != 0 {
        (*self.0.add(1).cast::<sys::os_mbuf_pkthdr>()).omp_len
      } else {
        self.len()
      }
    }
  }

  #[inline]
  pub(crate) fn append(&mut self, data: &[u8]) -> c_int {
    if self.is_null() {
      return 2; // BLE_HS_EINVAL is usually 2
    }
    unsafe { _os_mbuf_append(self.0, data.as_ptr() as _, data.len() as _) }
  }

  #[inline]
  pub fn from_slice(buf: &[u8]) -> Option<Self> {
    let mbuf = unsafe { sys::ble_hs_mbuf_from_flat(buf.as_ptr() as _, buf.len() as _) };
    if mbuf.is_null() {
      None
    } else {
      Some(OsMBuf(mbuf))
    }
  }

  #[inline]
  pub fn iter(&self) -> OsMBufIterator {
    OsMBufIterator(*self)
  }

  #[inline]
  fn has_next(&self) -> bool {
    if self.is_null() {
      return false;
    }
    unsafe { !(*self.0).om_next.sle_next.is_null() }
  }

  #[inline]
  pub fn as_flat(&self) -> FlatData<'_> {
    if self.has_next() {
      let mut buf = Vec::with_capacity(self.entire_len() as _);

      for mbuf in self.iter() {
        buf.extend_from_slice(mbuf.as_slice());
      }

      FlatData::Vec(buf)
    } else {
      FlatData::Slice(self.as_slice())
    }
  }

  #[inline]
  pub fn as_raw(&self) -> *mut sys::os_mbuf {
    self.0
  }
}

pub(crate) struct OsMBufIterator(OsMBuf);

impl Iterator for OsMBufIterator {
  type Item = OsMBuf;

  fn next(&mut self) -> Option<Self::Item> {
    if self.0.is_null() {
      None
    } else {
      let current = self.0;

      self.0 = OsMBuf(unsafe { (*self.0.0).om_next.sle_next });

      Some(current)
    }
  }
}

pub(crate) enum FlatData<'a> {
  Vec(Vec<u8>),
  Slice(&'a [u8]),
}

impl FlatData<'_> {
  pub fn as_slice(&self) -> &[u8] {
    match self {
      FlatData::Vec(v) => v.as_slice(),
      FlatData::Slice(s) => s,
    }
  }
}
