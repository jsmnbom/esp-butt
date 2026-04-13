use std::time::Instant;

use crate::{
  app::{App, MAIN_FONT, NavigationEvent, SMALL_FONT, ALT_SMALL_FONT},
  utils::{
    draw::*,
  },
};

#[derive(Debug)]
pub struct DeviceListState {
  /// Cursor position into `devices`
  pub cursor: usize,
  /// Time of last user interaction, used to drive the idle timeout.
  pub last_activity: Instant,
  pub scan_indicator: u8,
}

impl Default for DeviceListState {
  fn default() -> Self {
    Self {
      cursor: 0,
      last_activity: Instant::now(),
      scan_indicator: 0,
    }
  }
}

impl App {
  pub async fn on_device_list_navigation(
    &mut self,
    state: &mut DeviceListState,
    nav_event: &NavigationEvent,
  ) {
    state.last_activity = Instant::now();

    if self.devices().is_empty() {
      state.cursor = 0;
      return;
    }

    if state.cursor >= self.devices().len() {
      state.cursor = self.devices().len().saturating_sub(1);
    }

    match nav_event {
      NavigationEvent::Up => {
        if state.cursor > 0 {
          state.cursor -= 1;
          log::debug!("Cursor moved up to {}", state.cursor);
          self.queue_draw();
        }
      }
      NavigationEvent::Down => {
        if (state.cursor + 1) < self.devices().len() {
          state.cursor += 1;
          log::debug!("Cursor moved down to {}", state.cursor);
          self.queue_draw();
        }
      }
      NavigationEvent::Select => {
        let device_index = state.cursor;
        self.connect_device(device_index).await;
      }
    }
  }

  pub async fn on_device_list_tick(&mut self, state: &mut DeviceListState) {
    state.scan_indicator = (state.scan_indicator + 1) % 3;
    if state.last_activity.elapsed() >= std::time::Duration::from_secs(60) {
      log::info!("Device list idle timeout, returning to idle");
      self.goto_idle().await;
    }

    self.ensure_scanning().await;

    self.queue_draw();
  }

  pub fn draw_device_list(&self, state: &DeviceListState) -> anyhow::Result<()> {
    let mut screen = self.screen();
    let screen = &mut *screen;
    let count = self.devices().len();
    // Figure out which 3 devices to show based on the cursor position
    let offset = if count <= 3 {
      0
    } else if state.cursor == 0 {
      0
    } else if state.cursor == (count - 1) {
      count - 3
    } else {
      state.cursor - 1
    };
    for i in 0..3 {
      let x = offset + i;
      let device = &self.devices().get(x);
      ListItem {
        y: 6 + i as i32 * 19,
        name: device.map(|d| d.name()),
        address: device.map(|d| d.address()),
        rssi: device.and_then(|d| d.rssi()),
        first: x == 0,
        last: x == count.saturating_sub(1),
        selected: x == state.cursor,
      }
      .draw(screen)?;
    }

     Text::new(
        &format!("SCANNING... ({} FOUND)", count),
        Point::new(64,7),
        &ALT_SMALL_FONT,
      )
      .align(HorizontalAlignment::Center)
      .vertical_pos(VerticalPosition::Bottom)
      .draw(screen)?;

    ControllerBattery {
      point: Point::new(119, 0),
      level: self.battery(),
    }
    .draw(screen)?;

    ScanIndicator {
      point: Point::new(0, 0),
      state: state.scan_indicator as i32,
    }
    .draw(screen)?;

    Ok(())
  }
}

pub struct ListItem<'a> {
  pub y: i32,
  pub name: Option<&'a str>,
  pub address: Option<&'a str>,
  pub rssi: Option<i8>,
  pub first: bool,
  pub last: bool,
  pub selected: bool,
}

impl Drawable for ListItem<'_> {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    static CONTAINER: Styled<RoundedRectangle, PrimitiveStyle<BinaryColor>> = Styled::new(
      RoundedRectangle::new(
        Rectangle::new(Point::new(0, 0), Size::new(128, 20)),
        CornerRadiiBuilder::new().all(Size::new(4, 4)).build(),
      ),
      PrimitiveStyle::with_stroke(BinaryColor::On, 1),
    );
    CONTAINER.translate(Point::new(0, self.y)).draw(target)?;

    for x in (0..128).step_by(4) {
      if !self.first {
        Pixel(Point::new(x, self.y), BinaryColor::Off).draw(target)?;
      }
      if !self.last {
        Pixel(Point::new(x, self.y + 19), BinaryColor::Off).draw(target)?;
      }
    }

    if let Some(name) = self.name {
      Text::new(name, Point::new(16, self.y + 2), &MAIN_FONT).draw(target)?;
    }
    if let Some(address) = self.address {
      Text::new(address, Point::new(16, self.y + 11), &SMALL_FONT).draw(target)?;
    }

    if let Some(rssi) = self.rssi {
      SignalStrengthBar {
        point: Point::new(108, self.y + 3),
        rssi,
      }
      .draw(target)?;
    }
    if self.name.is_some() {
      if self.selected {
        Triangle::new(
          Point::new(6, self.y + 6),
          Point::new(9, self.y + 9),
          Point::new(6, self.y + 12),
        )
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target)?;
      } else {
        Rectangle::new(Point::new(6, self.y + 8), Size::new(3, 3))
          .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
          .draw(target)?;
      }
    }

    Ok(())
  }
}

pub struct ScanIndicator {
  pub point: Point,
  pub state: i32,
}

impl Drawable for ScanIndicator {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    let points: [Point; 7] = [
      Point::new(0, 4),
      Point::new(0, 3),
      Point::new(3, 0),
      Point::new(5, 0),
      Point::new(5, 1),
      Point::new(2, 4),
      Point::new(0, 4),
    ];

    for i in 0i32..3i32 {
      Polyline::new(&points)
        .translate(self.point + Point::new(i * 5, 0))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;

      if i == self.state {
        for j in 0i32..2i32 {
          Line::new(
            self.point + Point::new(i * 5 + 1 + j, 3),
            self.point + Point::new(i * 5 + 3 + j, 1),
          )
          .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
          .draw(target)?;
        }
      }
    }
    Ok(())
  }
}
