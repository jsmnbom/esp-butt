mod app;
mod buttplug;
mod hw;
mod img;
mod utils;

#[cfg(target_os = "espidf")]
mod ble;

#[cfg(target_os = "espidf")]
fn main() -> anyhow::Result<()> {
  use futures_concurrency::prelude::*;

  esp_idf_svc::sys::link_patches();

  tracing_log::LogTracer::init().unwrap();

  utils::log::Subscriber::new(tracing::Level::DEBUG)
    .with_filter("esp_idf_svc::timer", tracing::Level::WARN)
    .with_filter("buttplug", tracing::Level::DEBUG)
    .install();

  hw::init()?;
  ble::init();
  buttplug::init();

  // utils::report::start_reporting(core::time::Duration::from_secs(5));

  log::info!("Hello, world!");

  let peripherals = esp_idf_svc::hal::peripherals::Peripherals::take().unwrap();

  let display = hw::Display::new(
    peripherals.i2c0,
    peripherals.pins.gpio5,
    peripherals.pins.gpio6,
  )?;

  let adc = hw::AdcInputs::new(
    peripherals.adc1,
    (peripherals.pins.gpio1, peripherals.pins.gpio2),
    peripherals.pins.gpio3,
  )?;

  let encoder = hw::Encoder::new(
    peripherals.pins.gpio9,
    peripherals.pins.gpio8,
    peripherals.pins.gpio7,
  )?;

  let ticker = hw::Ticker::new()?;

  let input_event_stream = Box::pin((adc.stream(), encoder.stream(), ticker.stream()).merge());

  let app = app::App::new(display, adc);

  esp_idf_svc::hal::task::block_on(async move { app.main(input_event_stream).await })
}

#[cfg(not(target_os = "espidf"))]
#[global_allocator]
static GLOBAL: tracing_tracy::client::ProfiledAllocator<std::alloc::System> =
  tracing_tracy::client::ProfiledAllocator::new(std::alloc::System, 100);

#[cfg(not(target_os = "espidf"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
  use tracing_subscriber::layer::SubscriberExt;

  tracing_log::LogTracer::init()?;

  tracy_client::register_demangler!();

  tracing::subscriber::set_global_default(
    tracing_subscriber::fmt()
      .with_max_level(tracing::Level::DEBUG)
      .with_writer(std::io::stderr)
      .finish()
      .with(tracing_tracy::TracyLayer::default()),
  )?;

  buttplug::init();

  log::info!("Hello, world!");

  let hw::HardwareMock {
    display,
    input_stream,
  } = hw::HardwareMock::new()?;

  let app = app::App::new(display);

  app.main(input_stream).await
}
