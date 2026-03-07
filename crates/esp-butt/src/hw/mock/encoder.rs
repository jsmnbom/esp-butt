use crossterm::event::{Event, EventStream, KeyCode};
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::{app::{AppEvent, NavigationEvent}, utils};


enum State {
  Idle,
  Slider,
}

pub struct Encoder {
  tx: tokio::sync::broadcast::Sender<AppEvent>,
}

impl Encoder {
  pub fn new() -> anyhow::Result<Self> {
    let (tx, _) = broadcast::channel(16);

    Self::spawn_encoder_task(tx.clone());

    Ok(Self { tx })
  }

  pub fn stream(&self) -> impl futures::Stream<Item = AppEvent> + use<> {
    utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe())
  }

  fn spawn_encoder_task(tx: broadcast::Sender<AppEvent>) {
    utils::task::spawn(
      async move {
        let mut reader = EventStream::new();
        let mut state = State::Idle;
        let mut slider = 0;
        let mut slider_value = String::new();

        loop {
          match reader.next().await {
            Some(Ok(event)) => match event {
              Event::Key(key) => {
                if key.code == KeyCode::Esc {
                  log::info!("Escape key pressed, sending Quit event");
                  tx.send(AppEvent::Quit).ok();
                  continue;
                }

                if let Some(event) = match key.code {
                  KeyCode::Up => Some(AppEvent::Navigation(NavigationEvent::Up)),
                  KeyCode::Down => Some(AppEvent::Navigation(NavigationEvent::Down)),
                  KeyCode::Enter => Some(AppEvent::Navigation(NavigationEvent::Select)),
                  _ => None,
                } {
                  tx.send(event).ok();
                }

                match state {
                  State::Idle => {
                    if let KeyCode::Char(c) = key.code {
                      if c.is_ascii_digit() {
                        slider = c.to_digit(10).unwrap_or(0);
                        log::info!("Selected slider {}", slider);
                        state = State::Slider;
                      }
                    }
                  }
                  State::Slider => {
                    if let KeyCode::Char(c) = key.code {
                      if c.is_ascii_digit() {
                        slider_value.push(c);
                      } else if c == ' ' {
                        if let Ok(value) = slider_value.parse::<u16>() {
                          log::info!("Setting slider {} to value {}", slider, value);
                          tx.send(AppEvent::SliderChanged(slider as u8, value)).ok();
                        }
                        slider_value.clear();
                        state = State::Idle;
                      }
                    }
                  }
                }
              }

              _ => {}
            },
            Some(Err(e)) => log::error!("Error reading event: {e}"),
            None => break,
          }
        }
      },
      c"Encoder Task",
      4096,
      utils::task::Core::Pro,
      5,
    );
  }
}
