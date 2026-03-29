use embedded_graphics::{
  image::Image,
  pixelcolor::BinaryColor,
  prelude::*,
  primitives::{PrimitiveStyle, Rectangle, RoundedRectangle},
};

use crate::{app::{NavigationEvent, SMALL_FONT}, img, utils};

use super::App;

impl App {
  pub fn on_idle_navigation(&mut self, nav_event: &NavigationEvent) {
    match nav_event {
        NavigationEvent::Up | NavigationEvent::Down | NavigationEvent::Select => {
          self.goto_device_list();
        }
      }
  }

  pub fn draw_idle(&mut self) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    Image::new(&img::LOGO, Point::new(32, 0)).draw(screen)?;

    if self.boot_loading {
      RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(100, 24), Size::new(24, 16)),
        Size::new(3, 3),
      )
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(screen)?;

      utils::draw::draw_text(screen, &SMALL_FONT, "LOAD", Point::new(103, 29))?;
    }

    Ok(())
  }
}
