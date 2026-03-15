use embedded_graphics::{Pixel, pixelcolor::BinaryColor, prelude::*};

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
const BUFFER_SIZE: usize = WIDTH as usize * HEIGHT as usize / 8;
const OFFSET: u32 = 0;

pub type DisplayCanvas = Canvas<BUFFER_SIZE, WIDTH, HEIGHT, OFFSET>;
type SubmitFrame = Box<dyn Fn(&[u8]) -> anyhow::Result<()> + Send>;

pub struct Display {
  canvas: DisplayCanvas,
  submit_frame: SubmitFrame,
}

impl Display {
  pub fn new(
    submit_frame: impl Fn(&[u8]) -> anyhow::Result<()> + Send + 'static,
  ) -> anyhow::Result<Self> {
    Ok(Self {
      canvas: Canvas::new(),
      submit_frame: Box::new(submit_frame),
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
    (self.submit_frame)(self.canvas.get_buffer())
  }
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
