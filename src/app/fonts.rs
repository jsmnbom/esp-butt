use std::sync::LazyLock;

use u8g2_fonts::{FontRenderer, fonts};

pub static SMALL_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_tiny5_te>);

pub static ALT_SMALL_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_04b_03_tr>);

pub static MAIN_FONT: LazyLock<FontRenderer> =
  LazyLock::new(FontRenderer::new::<fonts::u8g2_font_haxrcorp4089_tr>);
