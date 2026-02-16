use std::ffi::CStr;

use esp_idf_svc::hal::{
  cpu::Core,
  task::{
    block_on,
    thread::{MallocCap, ThreadSpawnConfiguration},
  },
};

pub const PRO_CORE: Core = Core::Core0;
pub const APP_CORE: Core = Core::Core1;

pub fn spawn<F>(future: F, name: &'static CStr, stack_size: usize, core: Core)
where
  F: std::future::Future<Output = ()> + Send + 'static,
{
  pre_spawn(name, stack_size, core);
  std::thread::spawn(move || block_on(future));
}

pub fn pre_spawn(name: &'static CStr, stack_size: usize, core: Core) {
  ThreadSpawnConfiguration {
    name: Some(name.to_bytes_with_nul()),
    stack_size,
    pin_to_core: Some(core),
    stack_alloc_caps: MallocCap::Cap8bit | MallocCap::Internal,
    ..Default::default()
  }
  .set()
  .expect("Failed to set thread spawn configuration");

  log::info!(
    "Spawning task '{}' on core {:?} with stack size {}",
    name.to_str().unwrap_or("invalid UTF-8"),
    core,
    stack_size
  );
}
