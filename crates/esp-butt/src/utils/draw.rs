use embedded_graphics::{pixelcolor::BinaryColor, prelude::Point};
use u8g2_fonts::{
  FontRenderer,
  types::{FontColor, HorizontalAlignment, VerticalPosition},
};

use crate::hw;

pub fn draw_text(
  canvas: &mut hw::DisplayCanvas,
  font: &FontRenderer,
  text: &str,
  position: Point,
) -> anyhow::Result<()> {
  font
    .render_aligned(
      text,
      position,
      VerticalPosition::Top,
      HorizontalAlignment::Left,
      FontColor::Transparent(BinaryColor::On),
      canvas,
    )
    .map_err(|_| anyhow::anyhow!("Failed to render text: {}", text))?;

  Ok(())
}
