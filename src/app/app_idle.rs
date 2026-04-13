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

  pub fn draw_idle(&self) -> anyhow::Result<()> {
    let mut screen = self.screen();

    Image::new(&img::LOGO, Point::new(32, 0)).draw(&mut *screen)?;

    ControllerBattery {
      point: Point::new(119, 0),
      level: self.battery(),
    }
    .draw(&mut *screen)?;

    if self.loading() {
      RoundedRectangle::with_equal_corners(
        Rectangle::new(Point::new(100, 24), Size::new(24, 14)),
        Size::new(3, 3),
      )
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(&mut *screen)?;

      Text::new("LOAD", Point::new(102, 27), &SMALL_FONT).draw(&mut *screen)?;
    }

    Ok(())
  }
}
