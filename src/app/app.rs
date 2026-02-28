use std::sync::LazyLock;

use buttplug_client::ButtplugClientEvent;
use embedded_graphics::{
  image::Image,
  pixelcolor::BinaryColor,
  prelude::*,
  primitives::{
    CornerRadiiBuilder,
    PrimitiveStyle,
    PrimitiveStyleBuilder,
    Rectangle,
    RoundedRectangle,
    Styled,
    Triangle,
  },
};
use futures::stream::StreamExt;
use futures_concurrency::prelude::*;
use tokio::sync::broadcast;
use u8g2_fonts::{
  FontRenderer,
  fonts,
  types::{FontColor, HorizontalAlignment, VerticalPosition},
};

use crate::{
  app::{AppEvent, NavigationEvent},
  buttplug,
  hw,
  img,
  utils,
};

static SMALL_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_tiny5_te>);

static MAIN_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_haxrcorp4089_tr>);

#[derive(Debug)]
pub enum AppState {
  Idle,
  DeviceList { cursor: u16 },
  DeviceControl { device_index: usize },
}

pub struct AppBuilder {
  pub sliders: hw::Sliders,
  pub encoder: hw::Encoder,
  pub display: hw::Display,
}

impl AppBuilder {
  pub fn build(self) -> App {
    let (tx, _) = broadcast::channel(16);

    App {
      sliders: self.sliders,
      encoder: self.encoder,
      display: self.display,

      tx,

      client: buttplug_client::ButtplugClient::new("esp"),

      scanning: false,
      slider_values: [0; 2],
      devices: Vec::new(),
      state: AppState::Idle,
    }
  }
}

pub struct App {
  sliders: hw::Sliders,
  encoder: hw::Encoder,
  display: hw::Display,

  tx: broadcast::Sender<AppEvent>,

  client: buttplug_client::ButtplugClient,

  scanning: bool,
  slider_values: [u16; 2],
  devices: Vec<buttplug_client::ButtplugClientDevice>,
  state: AppState,
}

impl App {
  pub async fn main(mut self) -> anyhow::Result<()> {
    let connector = buttplug::create_buttplug()?;

    let client_stream = self
      .client
      .event_stream()
      .map(|event| AppEvent::ButtplugEvent(event));

    let self_stream = utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe());

    let mut event_stream = Box::pin(
      (
        client_stream,
        self_stream,
        self.sliders.stream(),
        self.encoder.stream(),
      )
        .merge(),
    );

    self.client.connect(connector).await?;

    self.queue_draw();

    while let Some(event) = event_stream.next().await {
      match event {
        AppEvent::ButtplugEvent(event) => match event {
          ButtplugClientEvent::DeviceAdded(device) => {
            log::info!("Device added: {}", device.name());
            self.devices.push(device);
            self.queue_draw();
          }
          ButtplugClientEvent::DeviceRemoved(device) => {
            log::info!("Device removed: {}", device.name());
            self.devices.retain(|d| d.index() != device.index());
            self.queue_draw();
          }
          ButtplugClientEvent::ServerDisconnect => {
            log::error!("Buttplug server disconnected");
            break;
          }
          ButtplugClientEvent::ScanningFinished => {
            log::info!("Buttplug scanning finished");
            self.scanning = false;
            self.queue_draw();
          }
          _ => {}
        },

        AppEvent::Navigation(nav_event) => self.on_navigation(nav_event).await,
        AppEvent::SliderChanged(index, value) => self.on_slider_changed(index, value).await,
        AppEvent::Draw => self.on_draw().await,
        #[cfg(not(target_os = "espidf"))]
        AppEvent::Quit => {
          log::info!("Quit event received, exiting");
          return Ok(());
        }
      }
    }

    Ok(())
  }

  fn set_state(&mut self, state: AppState) {
    self.state = state;
    self.queue_draw();
  }

  async fn on_navigation(&mut self, nav_event: NavigationEvent) {
    match self.state {
      AppState::Idle => match nav_event {
        NavigationEvent::Up | NavigationEvent::Down | NavigationEvent::Select => {
          self.set_state(AppState::DeviceList { cursor: 0 });
        }
      },
      AppState::DeviceList { cursor } => match nav_event {
        NavigationEvent::Up => {
          if cursor > 0 {
            self.set_state(AppState::DeviceList { cursor: cursor - 1 });
          }
        }
        NavigationEvent::Down => {
          if (cursor as usize) < self.devices.len() {
            self.set_state(AppState::DeviceList { cursor: cursor + 1 });
          }
        }
        NavigationEvent::Select => {
          if cursor == 0 {
            // Start/stop scanning
            if self.scanning {
              match self.client.stop_scanning().await {
                Ok(_) => {
                  self.scanning = false;
                  self.queue_draw();
                }
                Err(e) => log::error!("Error stopping scan: {:?}", e),
              }
            } else {
              match self.client.start_scanning().await {
                Ok(_) => {
                  self.scanning = true;
                  self.queue_draw();
                }
                Err(e) => log::error!("Error starting scan: {:?}", e),
              }
            }
          } else if (cursor as usize) <= self.devices.len() {
            // Go to device control screen
            self.set_state(AppState::DeviceControl {
              device_index: (cursor - 1) as usize,
            });
          }
        }
      },
      AppState::DeviceControl { device_index } => match nav_event {
        NavigationEvent::Up => {}
        NavigationEvent::Down => {}
        NavigationEvent::Select => {
          self.set_state(AppState::DeviceList { cursor: 0 });
        }
      },
    }
  }

  async fn on_slider_changed(&mut self, index: u8, value: u16) {
    // if index < self.state.sliders.len() as u8 {
    //   self.state.sliders[index as usize] = value;
    //   self.queue_draw();
    // }
  }

  fn queue_draw(&self) {
    if let Err(e) = self.tx.send(AppEvent::Draw) {
      log::error!("Error queueing draw event: {:?}", e);
    }
  }

  async fn on_draw(&mut self) {
    //log::info!("Drawing screen with state: {:?}", self.state);
    if let Err(e) = self.draw().await {
      log::error!("Error drawing: {:?}", e);
    }
  }

  async fn draw(&mut self) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();
    screen.get_mut_buffer().fill(0);

    match self.state {
      AppState::Idle => self.draw_idle()?,
      AppState::DeviceList { cursor } => self.draw_device_list(cursor)?,
      AppState::DeviceControl { device_index } => self.draw_device_control(device_index)?,
    }

    self.display.flush_all()?;

    Ok(())
  }

  fn draw_idle(&mut self) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    Image::new(&img::LOGO, Point::new(16, 0)).draw(screen)?;

    Ok(())
  }

  fn draw_device_list(&mut self, cursor: u16) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    static CONTAINER: Styled<RoundedRectangle, PrimitiveStyle<BinaryColor>> = Styled::new(
      RoundedRectangle::new(
        Rectangle::new(Point::new(0, 0), Size::new(120, 22)),
        CornerRadiiBuilder::new().all(Size::new(4, 4)).build(),
      ),
      PrimitiveStyle::with_stroke(BinaryColor::On, 1),
    );

    for y in [0, 21, 42] {
      CONTAINER.translate(Point::new(0, y)).draw(screen)?;
    }

    // Make the rounded rectangles have a striped line where they overlap
    for y in [21, 42] {
      for x in (0..120).step_by(4) {
        Pixel(Point::new(x, y), BinaryColor::Off).draw(screen)?;
      }
    }

    let item_height = 21;
    draw_text(
      screen,
      &MAIN_FONT,
      if self.scanning {
        "STOP SCAN"
      } else {
        "START SCAN"
      },
      Point::new(16, 3),
    )?;

    Rectangle::new(Point::new(16, 14), Size::new(85, 5))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(screen)?;

    for (index, item) in self.devices.iter().enumerate() {
      let y = 3 + (index as i32 + 1) * item_height;
      draw_text(screen, &MAIN_FONT, item.name().as_str(), Point::new(16, y))?;

      draw_text(
        screen,
        &SMALL_FONT,
        item.display_name().as_ref().map_or("", |s| s.as_str()),
        Point::new(16, y + 9),
      )?;
    }

    // Draw cursor
    for i in 0..self.devices.len() as u16 + 1 {
      // Draw cursor if cursor=i otherwise draw a 3x3 block
      let y = 3 + i as i32 * item_height;
      if i == cursor {
        Triangle::new(
          Point::new(6, y + 3),
          Point::new(9, y + 6),
          Point::new(6, y + 9),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(screen)?;
      } else {
        Rectangle::new(Point::new(7, y + 6), Size::new(3, 3))
          .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
          .draw(screen)?;
      }
    }

    Ok(())
  }

  fn draw_device_control(&mut self, device_index: usize) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    Ok(())
  }
}

fn draw_text<T: DrawTarget<Color = BinaryColor>>(
  screen: &mut T,
  font: &FontRenderer,
  text: &str,
  position: Point,
) -> anyhow::Result<()> {
  font
    .render_aligned(
      text,
      position,
      VerticalPosition::Top,
      HorizontalAlignment::Left,
      FontColor::Transparent(BinaryColor::On),
      screen,
    )
    .map_err(|_| anyhow::anyhow!("Failed to render text: {}", text))?;

  Ok(())
}

fn draw_slider<T: DrawTarget<Color = BinaryColor>>(
  screen: &mut T,
  x: i32,
  value: u16,
) -> Result<(), T::Error> {
  let slider_box_width = 7;
  let slider_inner_width = 3;

  let slider_box_height = 64;
  let slider_inner_height = 60;

  let slider_max_value = 4095;

  let slider_steps = 10; // 0, 1, 2, ..., 10

  let outline = PrimitiveStyleBuilder::new()
    .stroke_color(BinaryColor::On)
    .stroke_width(1)
    .build();

  RoundedRectangle::new(
    Rectangle::new(
      Point::new(x + 1, 0),
      Size::new(slider_box_width, slider_box_height),
    ),
    CornerRadiiBuilder::new().all(Size::new(1, 1)).build(),
  )
  .into_styled(outline)
  .draw(screen)?;

  let draw_tick_size = slider_inner_height as f32 / (slider_steps - 1) as f32;
  for step in 1..(slider_steps - 1) {
    let y = slider_inner_height - ((step as f32 * draw_tick_size).ceil() as i32) + 1;
    Pixel(Point::new(x, y), BinaryColor::On).draw(screen)?;
    Pixel(Point::new(x + 8, y), BinaryColor::On).draw(screen)?;
  }

  let value_step = slider_max_value as f32 / (slider_steps + 1) as f32;
  let value_draw_size = slider_inner_height as f32 / (slider_steps) as f32;

  for step in 1..=slider_steps {
    let step_val = ((step) as f32 * value_step).ceil() as u16;

    if value >= step_val {
      let y = 2 + slider_inner_height - ((step as f32 * value_draw_size).ceil() as i32);
      Rectangle::new(
        Point::new(x + 3, y),
        Size::new(slider_inner_width, value_draw_size.ceil() as u32),
      )
      .into_styled(
        PrimitiveStyleBuilder::new()
          .fill_color(BinaryColor::On)
          .build(),
      )
      .draw(screen)?;
    }
  }

  Ok(())
}
