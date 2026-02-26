use crossterm::event::{Event, EventStream, KeyCode};
pub use display::*;
use futures::StreamExt;
use tokio::sync::broadcast;

use crate::{
  app::{AppEvent, NavigationEvent},
  utils,
};

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
                  },
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
                  },
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
pub mod display {
  use std::io::{self, Write};

  use crossterm::{
    cursor::SavePosition,
    execute,
    terminal::{WindowSize, window_size},
  };
  use embedded_graphics::{Pixel, pixelcolor::BinaryColor, prelude::*};
use image::{DynamicImage, GenericImageView};

  const WIDTH: u32 = 128;
  const HEIGHT: u32 = 64;
  const BUFFER_SIZE: usize = WIDTH as usize * HEIGHT as usize / 8;

  pub struct Display {
    canvas: Canvas<BUFFER_SIZE, WIDTH, HEIGHT>,
    img: image::RgbImage,
  }

  impl Display {
    pub fn new() -> anyhow::Result<Self> {
      Ok(Self {
        canvas: Canvas::new(),
        img: image::RgbImage::new(WIDTH, HEIGHT),
      })
    }

    #[allow(dead_code)]
    pub fn get_canvas(&self) -> &Canvas<BUFFER_SIZE, WIDTH, HEIGHT> {
      &self.canvas
    }

    #[allow(dead_code)]
    pub fn get_mut_canvas(&mut self) -> &mut Canvas<BUFFER_SIZE, WIDTH, HEIGHT> {
      &mut self.canvas
    }

    #[allow(dead_code)]
    pub fn flush_all(&mut self) -> anyhow::Result<()> {
      self.flush()
    }

    fn max_display_size(&self) -> anyhow::Result<(u16, u16)> {
      let WindowSize { columns, rows, width, height } = window_size()?;
      let cell_size: (u16, u16) = (width / columns, height / rows);
      return Ok((cell_size.0 * columns, cell_size.1 * 10));
    }

    fn update_image(&mut self) {
      self
        .canvas
        .get_buffer()
        .iter()
        .enumerate()
        .for_each(|(i, byte)| {
          let x = (i as u32 % WIDTH) as u32;
          let y = (i as u32 / WIDTH) * 8;
          for bit in 0..8 {
            let pixel_on = (byte >> bit) & 1 == 1;
            let color = if pixel_on {
              image::Rgb([255, 255, 255])
            } else {
              image::Rgb([0, 0, 0])
            };
            if y + bit < HEIGHT {
              self.img.put_pixel(x, y + bit, color);
            }
          }
        });
    }

    pub fn flush(&mut self) -> anyhow::Result<()> {
      execute!(io::stdout(), SavePosition, crossterm::cursor::MoveTo(0, 0))?;
      io::stdout().flush()?;

      println!(
        "{}",
        kitty_image::WrappedCommand::new(kitty_image::Command::new(kitty_image::Action::Delete(
          kitty_image::ActionDelete {
            hard: true,
            target: kitty_image::DeleteTarget::Cursor
          }
        )))
      );

      self.update_image();
      let (max_width, max_height) = self.max_display_size()?;
      let img = DynamicImage::ImageRgb8(self.img.clone()).resize(max_width as u32, max_height as u32, image::imageops::FilterType::Nearest);
      let (width, height) = img.dimensions();

      let action = kitty_image::Action::TransmitAndDisplay(
        kitty_image::ActionTransmission {
          width,
          height,
          ..Default::default()
        },
        kitty_image::ActionPut {
          ..Default::default()
        },
      );

      let command = kitty_image::Command::with_payload_from_image(action, &img);
      let command = kitty_image::WrappedCommand::new(command);

      println!("{command}");
      io::stdout().flush()?;
      execute!(io::stdout(), crossterm::cursor::RestorePosition)?;

      Ok(())
    }
  }

  macro_rules! fast_mul {
    ($value:expr, $right:expr) => {{
      let value_u32 = ($value) as u32;
      if $right > 0 && ($right & ($right - 1)) == 0 {
        value_u32 << $right.trailing_zeros()
      } else {
        value_u32 * $right
      }
    }};
  }

  pub struct Canvas<const N: usize, const W: u32, const H: u32> {
    buffer: [u8; N],
  }

  impl<const N: usize, const W: u32, const H: u32> Canvas<N, W, H> {
    pub fn new() -> Self {
      Self { buffer: [0; N] }
    }

    const fn get_display_size(&self) -> (u32, u32) {
      (W, H)
    }

    #[allow(dead_code)]
    pub fn get_buffer(&self) -> &[u8; N] {
      &self.buffer
    }

    #[allow(dead_code)]
    pub fn get_mut_buffer(&mut self) -> &mut [u8; N] {
      &mut self.buffer
    }

    #[allow(dead_code)]
    pub fn set_pixel(&mut self, x: u32, y: u32, on: bool) {
      let idx = fast_mul!((y >> 3), W) + x; // y >> 3 is equal to y / 8
      let bit_mask = 1 << (y & 7); // y & 7 is equal to y % 8
      match on {
        true => self.buffer[idx as usize] |= bit_mask,
        false => self.buffer[idx as usize] &= !bit_mask,
      };
    }
  }

  impl<const N: usize, const W: u32, const H: u32> DrawTarget for Canvas<N, W, H> {
    type Color = BinaryColor;

    type Error = std::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
      I: IntoIterator<Item = Pixel<Self::Color>>,
    {
      let bb = self.bounding_box();

      pixels
        .into_iter()
        .filter(|Pixel(pos, _color)| bb.contains(*pos))
        .for_each(|Pixel(pos, color)| self.set_pixel(pos.x as u32, pos.y as u32, color.is_on()));

      Ok(())
    }
  }

  impl<const N: usize, const W: u32, const H: u32> OriginDimensions for Canvas<N, W, H> {
    fn size(&self) -> Size {
      let (width, height) = self.get_display_size();

      Size::new(width, height)
    }
  }
}
