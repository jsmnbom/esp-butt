use std::time::Instant;

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
  /// Cursor position into `devices`
  pub cursor: u16,
  /// Time of last user interaction, used to drive the idle timeout.
  pub last_activity: Instant,
}

impl App {
  pub async fn on_device_list_navigation(
    &mut self,
    state: &mut DeviceListState,
    nav_event: &NavigationEvent,
  ) {
    state.last_activity = Instant::now();

    if self.devices.is_empty() {
      state.cursor = 0;
      self.ensure_device_list_scanning().await;
      return;
    }

    if state.cursor as usize >= self.devices.len() {
      state.cursor = self.devices.len().saturating_sub(1) as u16;
    }

    match nav_event {
      NavigationEvent::Up => {
        if state.cursor > 0 {
          state.cursor -= 1;
          self.queue_draw();
        }
      }
      NavigationEvent::Down => {
        if (state.cursor as usize + 1) < self.devices.len() {
          state.cursor += 1;
          self.queue_draw();
        }
      }
      NavigationEvent::Select => {
        let device_index = state.cursor as usize;
        // If buttplug already has this device connected (e.g. user navigated
        // back without disconnecting), jump straight to device control.
        if self.devices[device_index].client_device().is_some() {
          self.current_device_index = Some(device_index);
          self.goto_device_control();
        } else {
          if self.scanning {
            log::info!("Stopping scan before connecting to device");
            match self.client.stop_scanning().await {
              Ok(_) => {
                self.scanning = false;
              }
              Err(e) => {
                log::error!("Error stopping scan before connect: {:?}", e);
                return;
              }
            }
          }

          self.pending_connect = Some(device_index);
          if let Err(e) = self.devices[device_index].connect().await {
            log::warn!("Error connecting selected device: {:?}", e);
            self.pending_connect = None;
          }
          self.queue_draw();
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

    for (index, item) in self.devices.iter().enumerate() {
      let y = 3 + index as i32 * item_height;
      let label = if self.pending_connect == Some(index) {
        format!("{} ...", item.name())
      } else {
        item.name().to_string()
      };
      utils::draw::draw_text(screen, &MAIN_FONT, &label, Point::new(16, y))?;

      if let Some(address) = item.address() {
        utils::draw::draw_text(screen, &SMALL_FONT, address, Point::new(16, y + 11))?;
      }
    }

    // Draw cursor
    for i in 0..self.devices.len() as u16 {
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
