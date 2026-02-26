use buttplug_client::ButtplugClientDevice;
use compact_str::CompactString;

#[derive(Debug)]
pub enum AppScreen {
  DeviceList { cursor: u16 },
  DeviceControl { device_index: usize },
}

impl Default for AppScreen {
  fn default() -> Self {
    AppScreen::DeviceList { cursor: 0 }
  }
}

#[derive(Debug, Default)]
pub struct AppState {
  pub scanning: bool,

  pub sliders: [u16; 2],

  pub devices: Vec<(CompactString, CompactString)>,

  pub screen: AppScreen,
}
