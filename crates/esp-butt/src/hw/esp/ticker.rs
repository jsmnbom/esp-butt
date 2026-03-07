use esp_idf_svc::sys::{self, esp};
use futures::Stream;
use tokio::sync::broadcast;

use crate::{app::AppEvent, esp_timer_create, utils};

/// Broadcast sender used as the argument to the ESP timer callback.
struct TickerArg {
  tx: broadcast::Sender<AppEvent>,
}

// Safety: the timer callback runs in the `esp_timer` task (not an ISR) so
// using a regular Mutex-backed broadcast::Sender is fine.
unsafe impl Send for TickerArg {}

/// Generates periodic [`AppEvent::Tick`] events via an ESP-IDF high-resolution timer.
pub struct Ticker {
  tx: broadcast::Sender<AppEvent>,
  timer_handle: sys::esp_timer_handle_t,
  arg_ptr: *mut TickerArg,
}

const TICK_INTERVAL_US: u64 = 60_000_000; // 60 seconds

impl Ticker {
  pub fn new() -> anyhow::Result<Self> {
    let (tx, _) = broadcast::channel(4);

    let arg = Box::into_raw(Box::new(TickerArg { tx: tx.clone() }));

    let timer_handle = esp_timer_create!(Ticker::timer_callback, ticker, arg)
      .map_err(|e| anyhow::anyhow!("Failed to create tick timer: {:?}", e))?;

    unsafe {
      sys::esp_timer_start_periodic(timer_handle, TICK_INTERVAL_US);
    }

    Ok(Self { tx, timer_handle, arg_ptr: arg })
  }

  fn timer_callback(arg: &mut TickerArg) {
    let _ = arg.tx.send(AppEvent::Tick);
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe())
  }
}

impl Drop for Ticker {
  fn drop(&mut self) {
    unsafe {
      sys::esp_timer_stop(self.timer_handle);
      sys::esp_timer_delete(self.timer_handle);
      drop(Box::from_raw(self.arg_ptr));
    }
  }
}
