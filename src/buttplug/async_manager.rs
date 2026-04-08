use std::ffi::CStr;

use buttplug_core::util::async_manager::AsyncManager;
use futures::{future::BoxFuture, task::FutureObj};
use tracing::Span;

use crate::utils;

#[derive(Default, Debug)]
pub struct EspAsyncManager;

impl AsyncManager for EspAsyncManager {
  fn spawn(&self, future: FutureObj<'static, ()>, span: Span) {
    let span_name: Option<&str> = span.metadata().and_then(|metadata| Some(metadata.name()));
    let mut name: &'static CStr = c"unnamed";
    let core = utils::task::Core::App;
    let mut stack_size = 12 * 1024;
    let priority = 5;

    match span_name {
      Some("ServerDeviceManager event loop") => {
        name = c"devicemgr";
      }
      Some("InProcessClientConnectorEventSenderLoop") => {
        name = c"connector";
        stack_size = 4 * 1024;
      }
      Some("ButtplugClient event loop") => {
        name = c"clientloop";
        stack_size = 8 * 1024;
      }
      Some("DeviceTask") => {
        name = c"devicecomm";
        stack_size = 8 * 1024;
      }
      Some("DeviceEventForwarding") => {
        name = c"deviceforward";
        stack_size = 8 * 1024;
      }
      Some("device creation") => {
        name = c"devicecreation";
        stack_size = 16 * 1024;
      }
      Some("deferred-comm") => {
        name = c"deferredcomm";
        stack_size = 4 * 1024;
      }
      Some("ble-hardware") => {
        name = c"blehw";
        stack_size = 8 * 1024;
      }
      Some("PingTimerDrop") => {
        name = c"pingdrop";
        stack_size = 8 * 1024;
      }
      Some("BtleplugAdapterTask") => {
        name = c"btleadapter";
      }
      Some("BtlePlugHardware Drop") => {
        name = c"btledrop";
      }
      _ => {}
    }

    if name == c"unnamed" {
      log::warn!(
        "Spawning task with unnamed span: {:?}. Consider adding a name to the span for better debugging.",
        span_name
      );
    }

    utils::task::spawn(future, name, stack_size, core, priority);
  }

  fn sleep(&self, duration: core::time::Duration) -> BoxFuture<'static, ()> {
    log::trace!("Sleeping for {:?}", duration);
    Box::pin(utils::task::sleep_timer_async(duration))
  }
}
