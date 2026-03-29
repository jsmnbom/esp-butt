use std::{ffi::CStr, time::Instant};

use async_trait::async_trait;
use buttplug_core::util::async_manager::AsyncManager;
use futures::task::FutureObj;
use tracing::Span;

use crate::utils;

#[derive(Default, Debug)]
pub struct EspAsyncManager;

#[async_trait]
impl AsyncManager for EspAsyncManager {
  fn spawn(&self, future: FutureObj<'static, ()>, span: Span) {
    let span_name: Option<&str> = span.metadata().and_then(|metadata| Some(metadata.name()));
    let mut name: &'static CStr = c"unnamed";
    let core = utils::task::Core::App;
    let mut stack_size = 12 * 1024;
    let priority = 5;

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
      Some("ClientLoop") => {
        name = c"clientloop";
      }
      Some("DeviceTask") => {
        name = c"devicecomm";
        stack_size = 8 * 1024;
      }
      Some("DeviceEventForwardingTask") => {
        name = c"deviceforward";
        stack_size = 8 * 1024;
      }
      Some("device creation") => {
        name = c"devicecreation";
        stack_size = 16 * 1024;
      },
      Some("deferred-comm") => {
        name = c"deferredcomm";
        stack_size = 8 * 1024;
      },
      Some("ble-hardware") => {
        name = c"blehw";
        stack_size = 8 * 1024;
      },
      Some("BtlePlugCommunicationManager::adapter_task") => {
        name = c"btleadapter";
        stack_size = 16 * 1024;
      },
      _ => {}
    }

    utils::task::spawn(future, name, stack_size, core, priority);
  }

  async fn sleep(&self, duration: core::time::Duration) {
    log::trace!("Sleeping for {:?}", duration);
    utils::task::sleep_timer_async(duration).await;
  }

  async fn sleep_until(&self, deadline: Instant) {
    let now = Instant::now();
    if deadline > now {
      let duration = deadline - now;
      log::trace!("Sleeping until {:?} (for {:?})", deadline, duration);
      utils::task::sleep_timer_async(duration).await;
    } else {
      log::trace!("Deadline {:?} already passed, not sleeping", deadline);
    }
  }
}
