use tokio::sync::broadcast;

use crate::{app::AppEvent, utils};

/// See ./src/hw/mock/encoder.rs for actual slider implementation.

pub const MAX_VALUE: u16 = 4095;

pub struct Sliders {
  tx: tokio::sync::broadcast::Sender<AppEvent>,
}

impl Sliders {
  pub fn new() -> anyhow::Result<Self> {
    let (tx, _) = broadcast::channel(16);
    Ok(Self { tx })
  }

  pub fn stream(&self) -> impl futures::Stream<Item = AppEvent> + use<> {
    utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe())
  }
}
