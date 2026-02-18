use std::ffi::{CStr, c_void};

use esp_idf_svc::hal::{
  cpu::Core,
  task::{block_on, create},
};

pub const PRO_CORE: Core = Core::Core0;
pub const APP_CORE: Core = Core::Core1;

pub fn spawn<F>(future: F, name: &'static CStr, stack_size: usize, core: Core, priority: u8)
where
  F: std::future::Future<Output = ()> + Send + 'static,
{
  log::debug!(
    "Spawning task '{}' on core {:?} with stack size {} and priority {}",
    name.to_str().unwrap_or("invalid UTF-8"),
    core,
    stack_size,
    priority
  );
  let arg = Box::into_raw(Box::new(future)) as *mut c_void;

  if let Err(e) = unsafe {
    create(
      task_handler::<F>,
      name,
      stack_size,
      arg,
      priority,
      Some(core),
    )
  } {
    log::error!("Failed to spawn thread '{}': {}", name.to_str().unwrap(), e);
    unsafe {
      drop(Box::from_raw(arg as *mut F));
    }
  }
}

extern "C" fn task_handler<F>(arg: *mut c_void)
where
  F: std::future::Future<Output = ()> + Send + 'static,
{
  let future = unsafe { Box::from_raw(arg as *mut F) };
  block_on(*future);
  unsafe {
    esp_idf_svc::sys::vTaskDelete(std::ptr::null_mut());
  }
}
