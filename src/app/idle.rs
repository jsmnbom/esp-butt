use super::App;
use super::fonts::SMALL_FONT;
use crate::{app::NavigationEvent, img, utils::draw::*};

impl App {
  pub async fn on_idle_navigation(&mut self, nav_event: &NavigationEvent) {
    match nav_event {
      NavigationEvent::Up | NavigationEvent::Down | NavigationEvent::Select => {
        self.goto_device_list().await;
      }
    }
  }

  pub fn draw_idle(&mut self) -> anyhow::Result<()> {
    let screen = self.display.get_mut_canvas();

    Image::new(&img::LOGO, Point::new(32, 0)).draw(screen)?;

    ControllerBattery {
      point: Point::new(0, 0),
      level: self.self_battery,
    }
    .draw(screen)?;

    if self.boot_loading {
      RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(100, 24), Size::new(24, 14)),
        Size::new(3, 3),
      )
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(screen)?;

      Text::new("LOAD", Point::new(102, 27), &SMALL_FONT).draw(screen)?;
    }

    Ok(())
  }
}
