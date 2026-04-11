use std::{
  ffi::c_void,
  sync::atomic::{AtomicBool, Ordering},
};

use esp_idf_svc::sys::{self, esp_nofail};
use log::{info, warn};

unsafe extern "C" {
  fn ble_store_config_init();
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static SYNCED: AtomicBool = AtomicBool::new(false);

pub fn init() {
  unsafe {
    let initialized = INITIALIZED.load(Ordering::Acquire);
    if !initialized {
      let result = sys::nvs_flash_init();
      if result == sys::ESP_ERR_NVS_NO_FREE_PAGES || result == sys::ESP_ERR_NVS_NEW_VERSION_FOUND {
        warn!("NVS initialisation failed. Erasing NVS.");
        esp_nofail!(sys::nvs_flash_erase());
        esp_nofail!(sys::nvs_flash_init());
      }

      sys::esp_bt_controller_mem_release(sys::esp_bt_mode_t_ESP_BT_MODE_CLASSIC_BT);

      sys::nimble_port_init();

      sys::ble_hs_cfg.sync_cb = Some(on_sync);
      sys::ble_hs_cfg.reset_cb = Some(on_reset);

      sys::ble_hs_cfg.sm_io_cap = sys::BLE_HS_IO_NO_INPUT_OUTPUT as _;
      #[allow(static_mut_refs)]
      sys::ble_hs_cfg.set_sm_bonding(0);
      #[allow(static_mut_refs)]
      sys::ble_hs_cfg.set_sm_mitm(0);
      #[allow(static_mut_refs)]
      sys::ble_hs_cfg.set_sm_sc(1);
      sys::ble_hs_cfg.sm_our_key_dist = 1 as _;
      sys::ble_hs_cfg.sm_their_key_dist = 3 as _;
      sys::ble_hs_cfg.store_status_cb = Some(sys::ble_store_util_status_rr);

      ble_store_config_init();

      sys::nimble_port_freertos_init(Some(ble_host_task));
    }

    loop {
      if SYNCED.load(Ordering::Acquire) {
        break;
      }
      sys::vPortYield();
    }

    INITIALIZED.store(true, Ordering::Release);
  }
}

extern "C" fn on_sync() {
  unsafe {
    sys::ble_hs_util_ensure_addr(0);

    // Prefer Coded PHY (LE Long Range) for both TX and RX on all new connections.
    // The controller will automatically negotiate a PHY update after connecting.
    // Devices that don't support Coded PHY will simply stay on 1M.
    let rc = sys::ble_gap_set_prefered_default_le_phy(
      sys::BLE_GAP_LE_PHY_CODED_MASK as u8,
      sys::BLE_GAP_LE_PHY_CODED_MASK as u8,
    );
    if rc != 0 {
      log::warn!("Failed to set preferred default PHY: {}", rc);
    }

    SYNCED.store(true, Ordering::Release);
  }
}

extern "C" fn on_reset(reason: i32) {
  info!("Resetting state; reason={reason}");
}

extern "C" fn ble_host_task(_: *mut c_void) {
  unsafe {
    info!("BLE Host Task Started");
    sys::nimble_port_run();
    sys::nimble_port_freertos_deinit();
  }
}
