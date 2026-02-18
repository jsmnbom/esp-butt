use std::{collections::HashMap, ffi::c_void};

use esp_idf_svc::sys::{self, esp_nofail};
use futures::executor::block_on;
use tokio::sync::broadcast;

use crate::{
  ble::{self, Address, Discovery, DiscoveryListener},
  utils,
};

pub fn run() {
  log::info!("Starting BLE discovery...");

  let (tx, mut rx) = broadcast::channel(16);

  let mut listener = Listener {
    peripherals: HashMap::new(),
    tx,
  };

  Discovery::new(&mut listener)
    .duration(core::time::Duration::from_secs(10))
    .start();

  block_on(async {
    let mut address: Option<Address> = None;
    while let Ok(peripheral) = rx.recv().await {
      log::info!("Received peripheral: {:?}", peripheral);

      if peripheral.name == "LVS-Z44226" {
        // if peripheral.name == "^.^" {
        address = Some(peripheral.address);
        break;
      }
    }

    if let Some(address) = address {
      log::info!("Found peripheral: {:?}", address);

      let connector = ble::ClientConnector::new(address);

      match connector.connect().await {
        Ok(client) => {
          log::info!("Connected to device!");

          for service in client.services_iter() {
            log::info!("Service: {:?}", service);
          }

          let test_service = client
            .get_service(uuid::uuid!("5a300001-0023-4bd4-bbd5-a6920e4c5653"))
            .unwrap();
          log::info!("Test service: {:?}", test_service);
          let tx_characteristic = test_service
            .get_characteristic(uuid::uuid!("5a300002-0023-4bd4-bbd5-a6920e4c5653"))
            .unwrap();
          let rx_characteristic = test_service
            .get_characteristic(uuid::uuid!("5a300003-0023-4bd4-bbd5-a6920e4c5653"))
            .unwrap();

          match rx_characteristic.subscribe().await {
            Ok(_) => log::info!("Subscribed to RX characteristic"),
            Err(e) => log::error!("Failed to subscribe to RX characteristic: {:?}", e),
          }

          let mut events = client.events();

          tokio::join!(
            async {
              while let Ok(event) = events.recv().await {
                match event {
                  Ok(ble::ClientEvent::Notification(n)) => {
                    log::info!("Received notification: {:?}", n);
                  }
                  Ok(ble::ClientEvent::Connected) => {
                    log::info!("Connected to device!");
                  }
                  Ok(ble::ClientEvent::Disconnected) => {
                    log::info!("Disconnected from device!");
                  }
                  Err(e) => {
                    log::error!("Failed to receive event: {:?}", e);
                  }
                }
              }
            },
            async {
              loop {
                utils::sleep(core::time::Duration::from_secs(5)).await;

                match rx_characteristic.subscribe().await {
                  Ok(_) => log::info!("Subscribed to RX characteristic"),
                  Err(e) => log::error!("Failed to subscribe to RX characteristic: {:?}", e),
                }

                utils::sleep(core::time::Duration::from_secs(5)).await;

                log::info!("Writing sanity check");

                if let Err(e) = tx_characteristic.write(b"DeviceType;").await {
                  log::error!("Failed to write to TX characteristic: {:?}", e);
                }
              }
            },
          );
        }
        Err(e) => log::error!("Failed to connect to device: {:?}", e),
      }
    } else {
      log::error!("No peripheral found!");
    }
  })
}

struct Listener {
  peripherals: HashMap<ble::Address, ble::PeripheralProperties>,
  tx: broadcast::Sender<ble::PeripheralProperties>,
}

impl DiscoveryListener for Listener {
  fn on_report(&mut self, report: &ble::AdReport) {
    log::info!("Received advertisement report: {:?}", report);
    let entry = self
      .peripherals
      .entry(report.address)
      .or_insert_with(|| ble::PeripheralProperties::new(report.address, report.rssi));
    if let Err(e) = entry.update(report) {
      log::warn!("Failed to update peripheral properties: {:?}", e);
    }
    self.tx.send(entry.clone()).ok();
  }

  fn on_complete(&mut self) {
    log::info!(
      "BLE discovery complete. Found {} peripherals.",
      self.peripherals.len()
    );
    for (address, properties) in self.peripherals.iter() {
      log::info!("Peripheral {:?}: {:?}", address, properties);
    }
  }
}
