mod ble;
mod buttplug;
mod utils;

use std::collections::HashMap;

use ble::{Discovery, DiscoveryListener};
use esp_idf_svc::sys;
use futures::executor::block_on;

fn setup() {
  esp_idf_svc::sys::link_patches();
  tracing_subscriber::fmt()
    .with_timer(utils::log::TimeOnlyTimer {})
    .init();

  unsafe {
    esp_idf_svc::sys::esp_vfs_eventfd_register(&esp_idf_svc::sys::esp_vfs_eventfd_config_t {
      max_fds: 4,
    });
  }
}

fn main() {
  setup();

  utils::heap::log_heap();

  ble::init();

  log::info!("Hello, world!");

  utils::heap::log_heap();

  // let mut listener = Listener {
  //   peripherals: HashMap::new(),
  // };

  // Discovery::new(&mut listener)
  //   .duration(core::time::Duration::from_secs(10))
  //   .start();

  log::info!("Starting BLE client...");

  let connector = ble::ClientConnector::new(ble::Address {
    kind: ble::AddrKind::RANDOM,
    addr: ble::BdAddr([0x71, 0xBA, 0xD4, 0x8E, 0x36, 0xE2]),
  });

  

  block_on(async {

    log::info!("Connecting to device...");

    match connector.connect().await {
      Ok(client) => log::info!("Connected to device!"),
      Err(e) => log::error!("Failed to connect to device: {:?}", e),
    }



    loop {
      async_io::Timer::after(core::time::Duration::from_secs(5)).await;
    }
  })
}

// struct Listener {
//   peripherals: HashMap<ble::Address, ble::PeripheralProperties>,
// }

// impl DiscoveryListener for Listener {
//   fn on_report(&mut self, report: &ble::AdReport) {
//     log::info!("Received advertisement report: {:?}", report);
//     let entry = self
//       .peripherals
//       .entry(report.address)
//       .or_insert_with(|| ble::PeripheralProperties::new(report.address, report.rssi));
//     if let Err(e) = entry.update(report) {
//       log::warn!("Failed to update peripheral properties: {:?}", e);
//     }
//   }

//   fn on_complete(&mut self) {
//     log::info!(
//       "BLE discovery complete. Found {} peripherals.",
//       self.peripherals.len()
//     );
//     for (address, properties) in self.peripherals.iter() {
//       log::info!("Peripheral {:?}: {:?}", address, properties);
//     }
//   }
// }
