use std::ffi::{CStr, c_void};

use async_trait::async_trait;
use buttplug_core::util::async_manager::AsyncManager;
use futures::task::FutureObj;
use tracing::Span;

use crate::utils::{
  self,
  spawn::{APP_CORE, PRO_CORE},
};

#[derive(Default, Debug)]
pub struct EspAsyncManager;

#[async_trait]
impl AsyncManager for EspAsyncManager {
  fn spawn(&self, future: FutureObj<'static, ()>, span: Span) {
    let span_name: Option<&str> = span.metadata().and_then(|metadata| Some(metadata.name()));
    let mut name: &'static CStr = c"unnamed";
    let mut core = APP_CORE;
    let mut stack_size = 12 * 1024;

    log::info!(
      "Spawning task in async manager with span name: {:?}",
      span_name
    );

    match span_name {
      Some("ServerDeviceManagerEventLoop") => {
        name = c"devicemgr";
        stack_size = 20 * 1024;
      }
      Some("InProcessClientConnectorEventSenderLoop") => {
        name = c"connector";
        stack_size = 8 * 1024;
      }
      Some("Client Loop Span") => {
        name = c"clientloop";
      }
      Some("DeviceCommunicationTask") => {
        name = c"devicecomm";
        stack_size = 8 * 1024;
        core = PRO_CORE;
      }
      Some("DeviceEventForwardingTask") => {
        name = c"deviceforward";
        stack_size = 8 * 1024;
      }
      Some("device creation") => {
        name = c"devicecreation";
        stack_size = 24 * 1024;
        core = PRO_CORE;
      }
      _ => {}
    }

    log::debug!(
      "Spawning task '{}' on core {:?} with stack size {}",
      name.to_str().unwrap_or("invalid UTF-8"),
      core,
      stack_size
    );

    let arg = Box::into_raw(Box::new(future)) as *mut c_void;

    if let Err(e) =
      unsafe { esp_idf_svc::hal::task::create(task_handler, name, stack_size, arg, 5, Some(core)) }
    {
      log::error!("Failed to spawn task '{}': {}", name.to_str().unwrap(), e);
      unsafe {
        drop(Box::from_raw(arg as *mut FutureObj<'static, ()>));
      }
    }

    // pre_spawn(name, stack_size, core);
    // std::thread::spawn(move || {
    //   esp_idf_svc::hal::task::block_on(future);
    // });
  }

  async fn sleep(&self, duration: core::time::Duration) {
    log::trace!("Sleeping for {:?}", duration);
    utils::sleep(duration).await;
  }
}

extern "C" fn task_handler(arg: *mut c_void) {
  let future = unsafe { Box::from_raw(arg as *mut FutureObj<'static, ()>) };
  esp_idf_svc::hal::task::block_on(*future);
  unsafe {
    esp_idf_svc::sys::vTaskDelete(std::ptr::null_mut());
  }
}
