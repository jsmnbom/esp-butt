pub use embedded_graphics::{
  Drawable,
  Pixel,
  draw_target::DrawTarget,
  geometry::{Point, Size},
  image::{Image, ImageRaw},
  pixelcolor::BinaryColor,
  transform::Transform,
  primitives::{
    CornerRadiiBuilder,
    Line,
    Polyline,
    Primitive,
    PrimitiveStyle,
    Rectangle,
    RoundedRectangle,
    Styled,
    Triangle,
  },
};
pub use u8g2_fonts::types::{HorizontalAlignment, VerticalPosition};
use u8g2_fonts::{FontRenderer, types::FontColor};

pub struct Text<'a> {
  pub content: &'a str,
  pub position: Point,
  pub font: &'a FontRenderer,
  pub vertical_pos: VerticalPosition,
  pub horizontal_align: HorizontalAlignment,
}

impl<'a> Text<'a> {
  pub fn new(content: &'a str, position: Point, font: &'a FontRenderer) -> Self {
    Self {
      content,
      position,
      font,
      vertical_pos: VerticalPosition::Top,
      horizontal_align: HorizontalAlignment::Left,
    }
  }

  pub fn vertical_pos(mut self, vertical_pos: VerticalPosition) -> Self {
    self.vertical_pos = vertical_pos;
    self
  }

  pub fn align(mut self, horizontal_align: HorizontalAlignment) -> Self {
    self.horizontal_align = horizontal_align;
    self
  }
}

impl<'a> Drawable for Text<'a> {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    match self.font.render_aligned(
      self.content,
      self.position,
      self.vertical_pos,
      self.horizontal_align,
      FontColor::Transparent(BinaryColor::On),
      target,
    ) {
      Ok(_) => Ok(()),
      Err(u8g2_fonts::Error::DisplayError(e)) => Err(e),
      Err(u8g2_fonts::Error::GlyphNotFound(e)) => {
        log::error!("Glyph not found for text '{}': {}", self.content, e);
        Ok(())
      }
      Err(u8g2_fonts::Error::BackgroundColorNotSupported) => unreachable!(),
    }
  }
}

pub struct ControllerBattery {
  pub point: Point,
  pub level: u8,
}

impl Drawable for ControllerBattery {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    Rectangle::new(self.point + Point::new(1, 0), Size::new(8, 4))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;
    Line::new(self.point + Point::new(0, 1), self.point + Point::new(0, 2))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;

    let fill_width = (self.level as u32 * 6) / 100; // 6 is the max fill width
    if fill_width > 0 {
      Rectangle::new(self.point + Point::new(1, 1), Size::new(fill_width, 2))
        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
        .draw(target)?;
    }

    Ok(())
  }
}

pub struct DeviceBattery {
  pub point: Point,
  pub level: u8,
}

impl Drawable for DeviceBattery {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    Rectangle::new(self.point, Size::new(18, 7))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;
    Line::new(
      self.point + Point::new(18, 1),
      self.point + Point::new(18, 3),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;

    let fill_width = (self.level as u32 * 10) / 100;
    // OO_OO_OO_OO_OO = 10
    // OO_O_________ = 3
    for i in 0i32..fill_width as i32 {
      let x = self.point.x + 2 + i + (i / 2); // Add extra space every 2 bars
      Line::new(
        Point::new(x, self.point.y + 2),
        Point::new(x, self.point.y + 4),
      )
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;
    }

    Ok(())
  }
}

pub struct SignalStrengthBar {
  pub point: Point,
  pub rssi: i8,
}

impl Drawable for SignalStrengthBar {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    // RSSI is between -127 and 20, but more realistically between -100 and -20 for our use case. We want to map that to a 1-5 columns of bars.
    let rssi = self.rssi.clamp(-100, -20);
    let num_bars = ((rssi + 100) as u8) / 16 + 1; // Map -100..-20 to 1..5
    let bottom_y = self.point.y + 12;
    let style = PrimitiveStyle::with_stroke(BinaryColor::On, 1);
    for i in 0..num_bars {
      let x = self.point.x + i as i32 * 3;
      let bar_height = (i + 1) as i32 * 2;
      // Left line: one pixel shorter (staircase effect)
      Line::new(
        Point::new(x, bottom_y - bar_height + 2),
        Point::new(x, bottom_y),
      )
      .into_styled(style)
      .draw(target)?;
      // Right line: full height
      Line::new(
        Point::new(x + 1, bottom_y - bar_height + 1),
        Point::new(x + 1, bottom_y),
      )
      .into_styled(style)
      .draw(target)?;
    }

    Ok(())
  }
}
