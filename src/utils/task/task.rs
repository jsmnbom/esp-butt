use std::ffi::{CStr, c_void};

use esp_idf_svc::{
  hal::{delay::Delay, task::create},
  sys,
  timer::EspTaskTimerService,
};

use super::Core;

impl From<Core> for esp_idf_svc::hal::cpu::Core {
  fn from(core: Core) -> Self {
    match core {
      Core::App => Self::Core0,
      Core::Pro => Self::Core1,
    }
  }
}

/// Spawn a new task to run the given future, with the specified name, stack size, core, and priority.
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
      Some(core.into()),
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
  esp_idf_svc::hal::task::block_on(*future);
  unsafe {
    sys::vTaskDelete(std::ptr::null_mut());
  }
}

/// Sleep for the specified duration using a timer, allowing other tasks to run.
pub async fn sleep_timer_async(duration: core::time::Duration) {
  EspTaskTimerService::new()
    .unwrap()
    .timer_async()
    .unwrap()
    .after(duration)
    .await
    .unwrap();
}

/// Sleep for the specified duration.
/// While this is a normally a blocking call, our async runtime uses a FreeRTOS
/// tasks per async task, so this will only block the current async task, not the entire core.
pub async fn sleep(duration: core::time::Duration) {
  Delay::new_default().delay_ms(duration.as_millis() as u32);
}
