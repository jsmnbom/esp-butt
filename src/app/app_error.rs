use crate::{
  app::{App, MAIN_FONT, NavigationEvent, SMALL_FONT},
  utils::draw::*,
};

impl App {
  /// Called when a select navigation event occurs in the Error state.
  /// Any select press returns to the device list.
  pub async fn on_error_navigation(&mut self, nav_event: &NavigationEvent) {
    match nav_event {
      NavigationEvent::Up | NavigationEvent::Down => {
        // do nothing
      }
      NavigationEvent::Select => {
        self.remove_current_device();
        self.goto_device_list().await;
      }
    }
  }

  pub fn draw_error(&self, message: &str) -> anyhow::Result<()> {
    let mut screen = self.screen();
    let screen = &mut *screen;

    Text::new("Error!", Point::new(64, 6), &MAIN_FONT)
      .align(HorizontalAlignment::Center)
      .draw(screen)?;

    if let Some(device) = self.current_device() {
      Text::new(device.name(), Point::new(64, 16), &SMALL_FONT)
        .align(HorizontalAlignment::Center)
        .draw(screen)?;
    }

    Text::new(message, Point::new(64, 24), &SMALL_FONT)
      .align(HorizontalAlignment::Center)
      .draw(screen)?;

    Ok(())
  }
}
