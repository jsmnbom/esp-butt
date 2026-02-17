use std::ffi::{CStr, c_char};

use buttplug_client::ButtplugClientEvent;
use esp_idf_svc::{hal::task::block_on, sys};
use futures::StreamExt;
use log::info;
use utils::spawn::APP_CORE;

mod ble;
mod buttplug;
mod utils;

fn setup() {
  esp_idf_svc::sys::link_patches();
  tracing_subscriber::fmt()
    .with_max_level(tracing::Level::DEBUG)
    .with_timer(utils::log::TimeOnlyTimer {})
    .init();

  unsafe {
    esp_idf_svc::sys::esp_vfs_eventfd_register(&esp_idf_svc::sys::esp_vfs_eventfd_config_t {
      max_fds: 4,
    });
  }
}

fn main() -> anyhow::Result<()> {
  setup();

  utils::heap::log_heap();
  v_task_list();

  ble::init();
  buttplug::init();

  log::info!("Hello, world!");

  utils::spawn::spawn(
    async {
      loop {
        utils::heap::log_heap();
        v_task_list();
        async_io::Timer::after(core::time::Duration::from_secs(10)).await;
      }
    },
    c"report",
    8 * 1024,
    APP_CORE,
  );

  let (connector, client) = buttplug::create_buttplug().unwrap();

  let mut event_stream = client.event_stream();

  block_on(async {
    info!("client.connect(connector).await");
    client.connect(connector).await?;
    info!("Client connected successfully!");

    // Start scanning - this just sends a message and returns
    info!("Starting BLE scan...");
    client.start_scanning().await?;
    info!("BLE scan started.");

    // Listen for events indefinitely
    info!("Listening for device events...");
    while let Some(event) = event_stream.next().await {
      info!("Received event: {:?}", event);
      if let ButtplugClientEvent::DeviceAdded(device) = event {
        info!("Device {} connected!", device.name());
        // You can interact with the device here
      }
    }

    Ok::<(), anyhow::Error>(())
  })?;

  block_on(async {
    async_io::Timer::after(core::time::Duration::from_secs(5)).await;
  });

  Ok(())
}

fn v_task_list() {
  const BUFFER_SIZE: usize = 1024;
  let mut buffer =
    allocator_api2::vec::Vec::with_capacity_in(BUFFER_SIZE, utils::heap::ExternalMemory);
  let c_buffer: *mut c_char = buffer.as_mut_ptr() as *mut c_char;

  unsafe {
    sys::vTaskList(c_buffer);
  }

  let c_str = unsafe { CStr::from_ptr(c_buffer) };
  match c_str.to_str() {
    Ok(rust_string) => println!("Task list:\n{}", rust_string),
    Err(err) => eprintln!("Failed to convert CStr to &str: {}", err),
  }
}
