use std::ops::{Deref, DerefMut};

use esp_idf_svc::hal::gpio::{InputPin, OutputPin};
use esp_idf_svc::hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_svc::hal::units::*;
use mini_oled::prelude::*;

pub struct Display(Box<Sh1106<I2cInterface<I2cDriver<'static>>>>);

impl Display {
  pub fn new(
    i2c: impl I2c + 'static,
    sda: impl InputPin + OutputPin + 'static,
    scl: impl InputPin + OutputPin + 'static,
  ) -> anyhow::Result<Self> {
    let i2c_config = I2cConfig::new().baudrate(400.kHz().into());
    let i2c_driver = I2cDriver::new(i2c, sda, scl, &i2c_config)?;
    let i2c_interface = I2cInterface::new(i2c_driver, 0x3C);
    let mut display = Box::new(Sh1106::new(i2c_interface));

    display.init()?;
    display.set_rotation(DisplayRotation::Rotate180)?;
    display.set_contrast(0x40)?;
    display.get_mut_canvas().get_mut_buffer().fill(0);
    display.flush_all()?;

    Ok(Display(display))
  }
}

impl Deref for Display {
  type Target = Sh1106<I2cInterface<I2cDriver<'static>>>;

  fn deref(&self) -> &Self::Target {
    &self.0
  }
}

impl DerefMut for Display {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.0
  }
}
