use embedded_graphics::{
  pixelcolor::BinaryColor,
  prelude::*,
  primitives::{CornerRadiiBuilder, PrimitiveStyle, Rectangle, RoundedRectangle, Styled, Triangle},
};

use crate::{
  app::{App, MAIN_FONT, NavigationEvent, SMALL_FONT},
  utils,
};

#[derive(Debug)]
pub struct DeviceListState {
  /// Cursor position: 0 = "START/STOP SCAN", 1..=devices.len() = device index + 1
  pub cursor: u16,
}

impl App {
  pub async fn on_device_list_navigation(
    &mut self,
    state: &mut DeviceListState,
    nav_event: &NavigationEvent,
  ) {
    match nav_event {
      NavigationEvent::Up => {
        if state.cursor > 0 {
          state.cursor -= 1;
          self.queue_draw();
        }
      }
      NavigationEvent::Down => {
        if (state.cursor as usize) < self.devices.len() {
          state.cursor += 1;
          self.queue_draw();
        }
      }
      NavigationEvent::Select => {
        if state.cursor == 0 {
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
        } else if (state.cursor as usize) <= self.devices.len() {
          // Go to device control screen
          self.goto_device_control((state.cursor - 1) as usize);
        }
      }
    }
  }

  pub fn draw_device_list(&mut self, state: &DeviceListState) -> anyhow::Result<()> {
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
    utils::draw::draw_text(
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
      let name: &str = item.name().as_str();
      utils::draw::draw_text(screen, &MAIN_FONT, name, Point::new(16, y))?;

      let display_name: &str = item.display_name().as_deref().unwrap_or("");
      utils::draw::draw_text(screen, &SMALL_FONT, display_name, Point::new(16, y + 9))?;
    }

    // Draw cursor
    for i in 0..self.devices.len() as u16 + 1 {
      // Draw cursor if cursor=i otherwise draw a 3x3 block
      let y = 3 + i as i32 * item_height;
      if i == state.cursor {
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
}
