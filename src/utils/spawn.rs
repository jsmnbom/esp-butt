use std::ffi::{CStr, c_void};

use esp_idf_svc::hal::{
  cpu::Core,
  task::{block_on, create},
};

pub const PRO_CORE: Core = Core::Core0;
pub const APP_CORE: Core = Core::Core1;

pub fn spawn<F>(future: F, name: &'static CStr, stack_size: usize, core: Core)
where
  F: std::future::Future<Output = ()> + Send + 'static,
{
  log::info!(
    "Spawning task '{}' on core {:?} with stack size {}",
    name.to_str().unwrap_or("invalid UTF-8"),
    core,
    stack_size
  );
  let arg = Box::into_raw(Box::new(future)) as *mut c_void;

  if let Err(e) = unsafe { create(task_handler::<F>, name, stack_size, arg, 5, Some(core)) } {
    log::error!("Failed to spawn thread '{}': {}", name.to_str().unwrap(), e);
    unsafe {
      drop(Box::from_raw(arg as *mut F));
    }
  }

  // pre_spawn(name, stack_size, core);
  // if let Err(e) = std::thread::Builder::new()
  //   .name(name.to_string_lossy().into_owned())
  //   .stack_size(stack_size)
  //   .spawn(move || block_on(future))
  // {
  //   log::error!("Failed to spawn thread '{}': {}", name.to_str().unwrap(), e);
  // }
}

extern "C" fn task_handler<F>(arg: *mut c_void)
where
  F: std::future::Future<Output = ()> + Send + 'static,
{
  let future = unsafe { Box::from_raw(arg as *mut F) };
  block_on(*future);
}

// pub fn pre_spawn(name: &'static CStr, stack_size: usize, core: Core) {
//   ThreadSpawnConfiguration {
//     name: Some(name.to_bytes_with_nul()),
//     stack_size,
//     pin_to_core: Some(core),
//     stack_alloc_caps: MallocCap::Cap8bit | MallocCap::Internal,
//     ..Default::default()
//   }
//   .set()
//   .expect("Failed to set thread spawn configuration");

//   log::info!(
//     "Spawning task '{}' on core {:?} with stack size {}",
//     name.to_str().unwrap_or("invalid UTF-8"),
//     core,
//     stack_size
//   );
// }
