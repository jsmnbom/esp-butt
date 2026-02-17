use std::{
  alloc::{GlobalAlloc, Layout},
  ptr::NonNull,
  sync::atomic::AtomicU32,
};

use allocator_api2::alloc::AllocError;
use esp_idf_svc::sys::{
  MALLOC_CAP_8BIT,
  MALLOC_CAP_INTERNAL,
  MALLOC_CAP_SPIRAM,
  heap_caps_aligned_alloc,
  heap_caps_free,
  heap_caps_get_free_size,
  heap_caps_get_largest_free_block,
  heap_caps_get_total_size,
  heap_caps_realloc,
};

#[global_allocator]
pub static HEAP: EspAlloc = EspAlloc::new(MALLOC_CAP_8BIT);

const BAR_WIDTH: usize = 35;

struct HeapAllocCapsGuard {}

impl HeapAllocCapsGuard {
  fn new(caps: u32) -> Self {
    HEAP.set_caps(caps);
    Self {}
  }
}

impl Drop for HeapAllocCapsGuard {
  fn drop(&mut self) {
    HEAP.set_caps(MALLOC_CAP_8BIT);
  }
}

pub struct EspAlloc {
  malloc_caps: AtomicU32,
}

impl EspAlloc {
  const fn new(caps: u32) -> Self {
    Self {
      malloc_caps: AtomicU32::new(caps),
    }
  }

  fn set_caps(&self, caps: u32) {
    self
      .malloc_caps
      .store(caps, std::sync::atomic::Ordering::SeqCst);
  }
}

unsafe impl Sync for EspAlloc {}

unsafe impl GlobalAlloc for EspAlloc {
  unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
    unsafe {
      heap_caps_aligned_alloc(
        layout.align(),
        layout.size(),
        self.malloc_caps.load(std::sync::atomic::Ordering::SeqCst),
      ) as *mut _
    }
  }

  unsafe fn realloc(&self, ptr: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
    unsafe {
      heap_caps_realloc(
        ptr as *mut _,
        new_size,
        self.malloc_caps.load(std::sync::atomic::Ordering::SeqCst),
      ) as *mut _
    }
  }

  unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
    unsafe {
      heap_caps_free(ptr as *mut _);
    }
  }
}

#[derive(Clone, Copy)]
pub struct ExternalMemory;

unsafe impl allocator_api2::alloc::Allocator for ExternalMemory {
  fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
    let raw_ptr = unsafe {
      heap_caps_aligned_alloc(
        layout.align(),
        layout.size(),
        MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT,
      ) as *mut _
    };
    let ptr = NonNull::new(raw_ptr).ok_or(AllocError)?;
    Ok(NonNull::slice_from_raw_parts(ptr, layout.size()))
  }

  unsafe fn deallocate(&self, ptr: NonNull<u8>, _layout: Layout) {
    unsafe {
      heap_caps_free(ptr.as_ptr() as *mut _);
    }
  }
}

pub fn with_spiram<F, T>(f: F) -> T
where
  F: FnOnce() -> T,
{
  let _guard = HeapAllocCapsGuard::new(MALLOC_CAP_SPIRAM | MALLOC_CAP_8BIT);
  f()
}

pub fn log_heap() {
  let stats = HeapStats::new();
  log::info!("Heap stats:\n{}", stats);
}

struct HeapRegionStats {
  total: usize,
  free: usize,
  largest_free: usize,
}

struct HeapStats {
  internal: HeapRegionStats,
  external: HeapRegionStats,
}

impl HeapStats {
  fn new() -> Self {
    Self {
      internal: HeapRegionStats {
        total: unsafe { heap_caps_get_total_size(MALLOC_CAP_INTERNAL) },
        free: unsafe { heap_caps_get_free_size(MALLOC_CAP_INTERNAL) },
        largest_free: unsafe { heap_caps_get_largest_free_block(MALLOC_CAP_INTERNAL) },
      },
      external: HeapRegionStats {
        total: unsafe { heap_caps_get_total_size(MALLOC_CAP_SPIRAM) },
        free: unsafe { heap_caps_get_free_size(MALLOC_CAP_SPIRAM) },
        largest_free: unsafe { heap_caps_get_largest_free_block(MALLOC_CAP_SPIRAM) },
      },
    }
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

impl std::fmt::Display for HeapStats {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    writeln!(f, "Internal RAM: {}", self.internal)?;
    writeln!(f, "External RAM: {}", self.external)
  }
}
