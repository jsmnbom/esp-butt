use std::{
  alloc::{GlobalAlloc, Layout},
  sync::atomic::{AtomicU32, Ordering},
};

use esp_idf_svc::sys;

// All allocations are 8-bit aligned
pub const MALLOC_CAP_INTERNAL: u32 = sys::MALLOC_CAP_8BIT | sys::MALLOC_CAP_INTERNAL;
pub const MALLOC_CAP_EXTERNAL: u32 = sys::MALLOC_CAP_8BIT | sys::MALLOC_CAP_SPIRAM;

#[global_allocator]
pub static HEAP: EspAlloc = EspAlloc::new(MALLOC_CAP_INTERNAL);

const BAR_WIDTH: usize = 35;

pub struct EspAlloc {
  malloc_caps: AtomicU32,
}

impl EspAlloc {
  const fn new(caps: u32) -> Self {
    Self {
      malloc_caps: AtomicU32::new(caps),
    }
  }

  #[allow(dead_code)]
  pub fn set_caps(&self, caps: u32) {
    self.malloc_caps.store(caps, Ordering::SeqCst);
  }

  /// Temporarily switch caps for the duration of the returned guard.
  /// Restores the previous caps when the guard is dropped.
  #[allow(dead_code)]
  pub fn use_caps(&self, caps: u32) -> CapsGuard<'_> {
    let prev = self.malloc_caps.swap(caps, Ordering::SeqCst);
    CapsGuard { heap: self, prev }
  }
}

#[allow(dead_code)]
pub struct CapsGuard<'a> {
  heap: &'a EspAlloc,
  prev: u32,
}

impl Drop for CapsGuard<'_> {
  fn drop(&mut self) {
    self.heap.malloc_caps.store(self.prev, Ordering::SeqCst);
  }
}

unsafe impl Sync for EspAlloc {}

unsafe impl GlobalAlloc for EspAlloc {
  #[inline(always)]
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    unsafe {
      sys::heap_caps_aligned_alloc(
        layout.align(),
        layout.size(),
        self.malloc_caps.load(Ordering::SeqCst),
      ) as *mut _
    }
  }

  #[inline(always)]
  unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
    unsafe {
      sys::heap_caps_realloc(
        ptr as *mut _,
        new_size,
        self.malloc_caps.load(Ordering::SeqCst),
      ) as *mut _
    }
  }

  #[inline(always)]
  unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
    unsafe { sys::heap_caps_free(ptr as *mut _) }
  }
}

pub struct HeapRegionStats {
  total: usize,
  free: usize,
  largest_free: usize,
}

impl HeapRegionStats {
  fn new(caps: u32) -> Self {
    Self {
      total: unsafe { sys::heap_caps_get_total_size(caps) },
      free: unsafe { sys::heap_caps_get_free_size(caps) },
      largest_free: unsafe { sys::heap_caps_get_largest_free_block(caps) },
    }
  }

  pub fn internal() -> Self {
    Self::new(MALLOC_CAP_INTERNAL)
  }

  pub fn external() -> Self {
    Self::new(MALLOC_CAP_EXTERNAL)
  }
}

fn write_bar(f: &mut core::fmt::Formatter<'_>, usage_percent: usize) -> core::fmt::Result {
  let used_blocks = BAR_WIDTH * usage_percent / 100;
  (0..used_blocks).try_for_each(|_| write!(f, "█"))?;
  (used_blocks..BAR_WIDTH).try_for_each(|_| write!(f, "░"))
}

impl std::fmt::Display for HeapRegionStats {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let used = self.total - self.free;
    let usage_percent = used * 100 / self.total;

    write_bar(f, usage_percent)?;

    write!(
      f,
      " | Used: {}% (Used {} of {}, free: {}, largest free block: {})",
      usage_percent, used, self.total, self.free, self.largest_free
    )
  }
}
