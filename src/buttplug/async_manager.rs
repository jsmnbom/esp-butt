use std::ffi::CStr;

use async_trait::async_trait;
use buttplug_core::util::async_manager::AsyncManager;
use futures::task::FutureObj;
use tracing::Span;

use crate::utils::spawn::{APP_CORE, PRO_CORE, pre_spawn};

#[derive(Default, Debug)]
pub struct EspAsyncManager;

#[async_trait]
impl AsyncManager for EspAsyncManager {
  fn spawn(&self, future: FutureObj<'static, ()>, span: Span) {
    let span_name: Option<&str> = span.metadata().and_then(|metadata| Some(metadata.name()));
    let mut name: &'static CStr = c"unnamed";
    let mut core = APP_CORE;
    // NOTE: Seems currently to be ignored in favor of CONFIG_PTHREAD_TASK_STACK_SIZE_DEFAULT
    let stack_size = 16 * 1024;

    log::info!(
      "Spawning task in async manager with span name: {:?}",
      span_name
    );

    match span_name {
      Some("ServerDeviceManagerEventLoop") => {
        name = c"devicemgr";
        core = PRO_CORE;
      }
      Some("InProcessClientConnectorEventSenderLoop") => {
        name = c"connector";
      }
      Some("Client Loop Span") => {
        name = c"clientloop";
      }
      _ => {}
    }

    pre_spawn(name, stack_size, core);
    std::thread::spawn(move || {
      esp_idf_svc::hal::task::block_on(future);
    });
  }

  async fn sleep(&self, duration: core::time::Duration) {
    async_io::Timer::after(duration).await;
  }
}
