use embedded_graphics::{
  image::Image,
  prelude::*,
};

use crate::{app::{AppState, NavigationEvent}, img};

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

    Image::new(&img::LOGO, Point::new(16, 0)).draw(screen)?;

    Ok(())
  }
}
