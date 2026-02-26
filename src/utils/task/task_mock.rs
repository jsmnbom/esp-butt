use std::ffi::CStr;

use tracing::{Instrument, info_span};

use super::Core;

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

  tokio::spawn(future.instrument(info_span!("task", name=name.to_str().unwrap_or("invalid UTF-8"))));
}

#[allow(dead_code)]
pub async fn sleep_timer_async(duration: core::time::Duration) {
  tokio::time::sleep(duration).await;
}

#[allow(dead_code)]
pub async fn sleep(duration: core::time::Duration) {
  tokio::time::sleep(duration).await;
}
