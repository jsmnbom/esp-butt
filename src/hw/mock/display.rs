use std::path::PathBuf;

use embedded_graphics::{Pixel, pixelcolor::BinaryColor, prelude::*};
use image::{ImageBuffer, Luma};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
const BUFFER_SIZE: usize = WIDTH as usize * HEIGHT as usize / 8;
const OFFSET: u32 = 0;

pub type DisplayCanvas = Canvas<BUFFER_SIZE, WIDTH, HEIGHT, OFFSET>;
type SubmitFrame = Box<dyn Fn(&[u8]) -> anyhow::Result<()> + Send>;

pub struct Display {
  canvas: DisplayCanvas,
  submit_frame: SubmitFrame,
  export_dir: Option<PathBuf>,
  last_frame: Vec<u8>,
  frame_count: u64,
}

impl Display {
  pub fn new(
    submit_frame: impl Fn(&[u8]) -> anyhow::Result<()> + Send + 'static,
  ) -> anyhow::Result<Self> {
    let export_dir = if std::env::var_os("ESP_BUTT_EXPORT_FRAMES").is_some() {
      let dir = std::env::temp_dir().join("esp-butt-frames");
      if dir.exists() {
        std::fs::remove_dir_all(&dir)
          .map_err(|e| anyhow::anyhow!("failed to clear frame export dir: {e}"))?;
      }
      std::fs::create_dir_all(&dir)
        .map_err(|e| anyhow::anyhow!("failed to create frame export dir: {e}"))?;
      eprintln!("frame export enabled: {}", dir.display());
      Some(dir)
    } else {
      None
    };

    Ok(Self {
      canvas: Canvas::new(),
      submit_frame: Box::new(submit_frame),
      export_dir,
      last_frame: Vec::new(),
      frame_count: 0,
    })
  }

  #[allow(dead_code)]
  pub fn get_canvas(&self) -> &DisplayCanvas {
    &self.canvas
  }

  #[allow(dead_code)]
  pub fn get_mut_canvas(&mut self) -> &mut DisplayCanvas {
    &mut self.canvas
  }

  #[allow(dead_code)]
  pub fn flush_all(&mut self) -> anyhow::Result<()> {
    self.flush()
  }

  pub fn flush(&mut self) -> anyhow::Result<()> {
    let buf = self.canvas.get_buffer();
    (self.submit_frame)(buf)?;
    if let Some(export_dir) = &self.export_dir {
      let buf = buf.as_slice();
      if buf != self.last_frame.as_slice() {
        self.last_frame.clear();
        self.last_frame.extend_from_slice(buf);
        let path = export_dir.join(format!("{:06}.png", self.frame_count));
        self.frame_count += 1;
        if let Err(e) = save_frame_png(buf, &path) {
          eprintln!("frame export failed: {e}");
        }
      }
    }
    Ok(())
  }
}

fn save_frame_png(buffer: &[u8], path: &PathBuf) -> anyhow::Result<()> {
  let mut img = ImageBuffer::<Luma<u8>, Vec<u8>>::new(WIDTH, HEIGHT);
  for x in 0..WIDTH {
    for page in 0..(HEIGHT / 8) {
      let byte_index = (page * WIDTH + x) as usize;
      let byte = buffer.get(byte_index).copied().unwrap_or_default();
      for bit in 0..8 {
        let y = page * 8 + bit;
        if y < HEIGHT {
          let on = ((byte >> bit) & 1) == 1;
          img.put_pixel(x, y, Luma([if on { 255u8 } else { 0u8 }]));
        }
      }
    }
  }
  img
    .save(path)
    .map_err(|e| anyhow::anyhow!("failed to save frame png: {e}"))
}

macro_rules! fast_mul {
  ($value:expr, $right:expr) => {{
    let value_u32 = ($value) as u32;
    if $right > 0 && ($right & ($right - 1)) == 0 {
      value_u32 << $right.trailing_zeros()
    } else {
      value_u32 * $right
    }
  }};
}

pub struct Canvas<const N: usize, const W: u32, const H: u32, const OFFSET: u32> {
  buffer: [u8; N],
}

impl<const N: usize, const W: u32, const H: u32, const O: u32> Canvas<N, W, H, O> {
  pub fn new() -> Self {
    Self { buffer: [0; N] }
  }

  const fn get_display_size(&self) -> (u32, u32) {
    (W, H)
  }

  #[allow(dead_code)]
  pub fn get_buffer(&self) -> &[u8; N] {
    &self.buffer
  }

  #[allow(dead_code)]
  pub fn get_mut_buffer(&mut self) -> &mut [u8; N] {
    &mut self.buffer
  }

  #[allow(dead_code)]
  pub fn set_pixel(&mut self, x: u32, y: u32, on: bool) {
    let idx = fast_mul!((y >> 3), W) + x; // y >> 3 is equal to y / 8
    let bit_mask = 1 << (y & 7); // y & 7 is equal to y % 8
    match on {
      true => self.buffer[idx as usize] |= bit_mask,
      false => self.buffer[idx as usize] &= !bit_mask,
    };
  }
}

impl<const N: usize, const W: u32, const H: u32, const O: u32> DrawTarget for Canvas<N, W, H, O> {
  type Color = BinaryColor;

  type Error = std::convert::Infallible;

  fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
  where
    I: IntoIterator<Item = Pixel<Self::Color>>,
  {
    let bb = self.bounding_box();

    pixels
      .into_iter()
      .filter(|Pixel(pos, _color)| bb.contains(*pos))
      .for_each(|Pixel(pos, color)| self.set_pixel(pos.x as u32, pos.y as u32, color.is_on()));

    Ok(())
  }
}

impl<const N: usize, const W: u32, const H: u32, const O: u32> OriginDimensions
  for Canvas<N, W, H, O>
{
  fn size(&self) -> Size {
    let (width, height) = self.get_display_size();

    Size::new(width, height)
  }
}
