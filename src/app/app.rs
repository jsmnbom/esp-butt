use std::sync::LazyLock;

use buttplug_client::ButtplugClientEvent;
use compact_str::CompactString;
use embedded_graphics::{
  pixelcolor::BinaryColor,
  prelude::*,
  primitives::{CornerRadiiBuilder, PrimitiveStyleBuilder, Rectangle, RoundedRectangle},
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
  app::{AppEvent, AppScreen, AppState, NavigationEvent},
  buttplug,
  hw,
  utils,
};

static SMALL_FONT: LazyLock<FontRenderer> =
  // LazyLock::new(FontRenderer::new::<fonts::u8g2_font_tiny5_te>);
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_fourmat_te>);

static MAIN_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_haxrcorp4089_tr>);

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

      state: AppState::default(),
    }
  }
}

pub struct App {
  sliders: hw::Sliders,
  encoder: hw::Encoder,
  display: hw::Display,

  tx: broadcast::Sender<AppEvent>,

  client: buttplug_client::ButtplugClient,

  state: AppState,
}

impl App {
  pub async fn main(mut self) -> anyhow::Result<()> {
    // let connector = buttplug::create_buttplug()?;

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


    self.state.devices.push(("Lovense Hush".into(), "A6:89:60:AA:EB:46".into()));
    self.state.devices.push(("Lovense Lush 3".into(), "BE:2C:92:D0:5B:C1".into()));
    self.state.devices.push(("Other device".into(), "6A:12:99:6E:56:95".into()));
    self.state.devices.push(("Another device".into(), "02:96:08:1F:26:4A".into()));

    // self.client.connect(connector).await?;

    self.queue_draw();

    while let Some(event) = event_stream.next().await {
      match event {
        AppEvent::ButtplugEvent(event) => match event {
          ButtplugClientEvent::DeviceAdded(device) => {
            log::info!("Device added: {}", device.name());
            // self.state.devices.push(device);
            self.queue_draw();
          }
          ButtplugClientEvent::DeviceRemoved(device) => {
            log::info!("Device removed: {}", device.name());
            // self.state.devices.retain(|d| d.index() != device.index());
            self.queue_draw();
          }
          ButtplugClientEvent::ServerDisconnect => {
            log::error!("Buttplug server disconnected");
            break;
          }
          ButtplugClientEvent::ScanningFinished => {
            log::info!("Buttplug scanning finished");
            self.state.scanning = false;
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

  async fn on_navigation(&mut self, nav_event: NavigationEvent) {
    match self.state.screen {
      AppScreen::DeviceList { cursor } => match nav_event {
        NavigationEvent::Up => {
          if cursor > 0 {
            self.state.screen = AppScreen::DeviceList { cursor: cursor - 1 };
            self.queue_draw();
          }
        }
        NavigationEvent::Down => {
          if (cursor as usize) < self.state.devices.len() {
            self.state.screen = AppScreen::DeviceList { cursor: cursor + 1 };
            self.queue_draw();
          }
        }
        NavigationEvent::Select => {
          if cursor == 0 {
            // Start/stop scanning
            if self.state.scanning {
              if let Err(e) = self.client.stop_scanning().await {
                log::error!("Error stopping scan: {:?}", e);
              }
            } else {
              if let Err(e) = self.client.start_scanning().await {
                log::error!("Error starting scan: {:?}", e);
              }
            }
          } else if (cursor as usize) <= self.state.devices.len() {
            // Go to device control screen
            self.state.screen = AppScreen::DeviceControl { device_index: (cursor - 1) as usize };
            self.queue_draw();
          }
        }
      },
      AppScreen::DeviceControl { device_index } => match nav_event {
        NavigationEvent::Up => {}
        NavigationEvent::Down => {}
        NavigationEvent::Select => {
          self.state.screen = AppScreen::DeviceList { cursor: 0 };
          self.queue_draw();
        }
      },
    }
  }

  async fn on_slider_changed(&mut self, index: u8, value: u16) {
    if index < self.state.sliders.len() as u8 {
      self.state.sliders[index as usize] = value;
      self.queue_draw();
    }
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

    match self.state.screen {
      AppScreen::DeviceList { cursor } => self.draw_device_list(cursor)?,
      AppScreen::DeviceControl { device_index } => self.draw_device_control(device_index)?,
      }
    


    self.display.flush_all()?;

    Ok(())
  }

  fn draw_device_list
  (
    &mut self,
    cursor: u16,
  ) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    // SMALL_FONT.render_aligned(
    //   "Devices:",
    //   Point::new(2, 10),
    //   VerticalPosition::Top,
    //    HorizontalAlignment::Left,
    //   FontColor::Transparent(BinaryColor::On),
      
     
    //   screen,
    // ).unwrap();

    let scan: CompactString = if self.state.scanning { "Stop Scanning".into() } else { "Start Scanning".into() };


   let items = std::iter::once((scan, "".into()))
      .chain(self.state.devices.iter().map(|d| (d.0.clone(), d.1.clone())))
      .enumerate();

    for (index, item) in items {
      let y = index as i32 * 16 ;
      MAIN_FONT.render_aligned(
        item.0.as_str(),
        Point::new(2, y),
        VerticalPosition::Top,
        HorizontalAlignment::Left,
        FontColor::Transparent(BinaryColor::On),
        screen,
      ).unwrap();

      SMALL_FONT.render_aligned(
        item.1.as_str(),
        Point::new(2, y+8),
        VerticalPosition::Top,
        HorizontalAlignment::Left,
        FontColor::Transparent(BinaryColor::On),
        screen,
      ).unwrap();
    }

    Ok(())
  }

  fn draw_device_control(
    &mut self,
    device_index: usize,
  ) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();
    
    Ok(())
  }


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
