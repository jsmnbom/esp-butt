use async_stream::stream;
use futures::Stream;
use std::time::Duration;

use crate::app::AppEvent;

pub struct Ticker;

impl Ticker {
  pub fn new() -> anyhow::Result<Self> {
    Ok(Self)
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    stream! {
      // Skip the first immediate tick
      tokio::time::sleep(Duration::from_secs(60)).await;
      loop {
        yield AppEvent::Tick;
        tokio::time::sleep(Duration::from_secs(60)).await;
      }
    }
  }
}
