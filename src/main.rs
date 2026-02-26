mod app;
mod buttplug;
mod utils;

#[cfg(target_os = "espidf")]
mod ble;

#[cfg(target_os = "espidf")]
mod hw;

#[cfg(not(target_os = "espidf"))]
#[path = "hw_mock/mod.rs"]
mod hw;

#[cfg(target_os = "espidf")]
fn setup_logging() {
  tracing_log::LogTracer::init().unwrap();

  utils::log::Subscriber::new(tracing::Level::DEBUG)
    .with_filter("esp_idf_svc::timer", tracing::Level::WARN)
    .with_filter("buttplug", tracing::Level::INFO)
    .install();
}

#[cfg(not(target_os = "espidf"))]
#[global_allocator]
static GLOBAL: tracing_tracy::client::ProfiledAllocator<std::alloc::System> =
  tracing_tracy::client::ProfiledAllocator::new(std::alloc::System, 100);

#[cfg(not(target_os = "espidf"))]
fn setup_logging() {
  use std::io;
  use std::io::Write;

  use tracing_subscriber::{fmt::MakeWriter, layer::SubscriberExt};

  tracing_log::LogTracer::init().unwrap();

  struct CarriageReturnWriter<W: Write> {
    writer: W,
  }

  impl<W: Write> Write for CarriageReturnWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
      match self.writer.write(buf) {
        Ok(n) => {
          // if we wrote the whole buffer, also write a carriage return to return to the beginning of the line
          if n == buf.len() {
            self.writer.write_all(b"\r")?;
          }
          Ok(n)
        }
        Err(e) => Err(e),
      }
    }

    fn flush(&mut self) -> io::Result<()> {
      self.writer.flush()
    }
  }

  struct CustomMakeWriter;

  impl<'a> MakeWriter<'a> for CustomMakeWriter {
    type Writer = CarriageReturnWriter<io::Stdout>;

    fn make_writer(&'a self) -> Self::Writer {
      CarriageReturnWriter {
        writer: io::stdout(),
      }
    }
  }

  tracing::subscriber::set_global_default(
    tracing_subscriber::fmt()
      .with_writer(CustomMakeWriter)
      .finish()
      .with(tracing_tracy::TracyLayer::default()),
  )
  .unwrap();
}

#[cfg(target_os = "espidf")]
fn main() -> anyhow::Result<()> {
  esp_idf_svc::sys::link_patches();
  setup_logging();

  ble::init();
  hw::init()?;
  // buttplug::init();

  // utils::report::start_reporting(core::time::Duration::from_secs(5));

  log::info!("Hello, world!");

  let peripherals = esp_idf_svc::hal::peripherals::Peripherals::take().unwrap();

  let sliders = hw::Sliders::new(
    peripherals.adc1,
    (peripherals.pins.gpio1, peripherals.pins.gpio2),
  )?;

  let encoder = hw::Encoder::new(
    peripherals.pins.gpio9,
    peripherals.pins.gpio8,
    peripherals.pins.gpio7,
  )?;

  let display = hw::Display::new(
    peripherals.i2c0,
    peripherals.pins.gpio5,
    peripherals.pins.gpio6,
  )?;

  let app = app::AppBuilder {
    sliders,
    encoder,
    display,
  }
  .build();

  esp_idf_svc::hal::task::block_on(async move { app.main().await })
}

#[cfg(not(target_os = "espidf"))]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
  use std::io::{self, Write};

  use crossterm::{
    execute,
    terminal,
    cursor
  };

  terminal::enable_raw_mode()?;

  execute!(
    io::stdout(),
    cursor::Hide,
    terminal::Clear(terminal::ClearType::All)
  )?;

  execute!(io::stdout(), cursor::MoveTo(0, 0))?;
  let (columns, rows) = terminal::size()?;
  write!(io::stdout(), "\x1B[12;{}r", rows)?;
  execute!(io::stdout(), cursor::MoveTo(0, 12))?;
  io::stdout().flush()?;

  setup_logging();

  buttplug::init();

  log::info!("Hello, world!");

  let sliders = hw::Sliders::new().unwrap();
  let encoder = hw::Encoder::new().unwrap();
  let display = hw::Display::new().unwrap();

  let app = app::AppBuilder {
    sliders,
    encoder,
    display,
  }
  .build();

  let result = app.main().await;

  execute!(
    io::stdout(),
    cursor::Show,
    terminal::Clear(terminal::ClearType::All)
  )?;
  write!(io::stdout(), "\x1B[r")?;
  terminal::disable_raw_mode()?;

  result
}
