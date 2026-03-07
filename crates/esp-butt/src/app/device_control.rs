use buttplug_client::device::{
  ClientDeviceCommandValue,
  ClientDeviceFeature,
  ClientDeviceOutputCommand,
};
use buttplug_core::{
  message::{DeviceFeatureOutput, InputType, OutputType},
  util::range::RangeInclusive,
};
use embedded_graphics::{
  pixelcolor::BinaryColor,
  prelude::*,
  primitives::{Line, PrimitiveStyle, Rectangle},
};
use litemap::LiteMap;

use crate::{
  app::{App, MAIN_FONT, NavigationEvent, SMALL_FONT, SliderEvent},
  hw,
  utils,
};

#[derive(Debug)]
pub struct DeviceControlOutput {
  feature: ClientDeviceFeature,
  step_count: u32,
  step_limit: RangeInclusive<i32>,
  value: u16,
  step: i32,
}

#[derive(Debug)]
pub struct DeviceControlState {
  /// Index into App.devices of the currently controlled device
  pub device_index: usize,
  // /// Cached battery level (0–100), refreshed on each Tick
  // pub battery: Option<u32>,
  // /// Cached RSSI value, refreshed on each Tick
  // pub rssi: Option<i8>,
  /// Slider index to output feature index mapping
  pub outputs: LiteMap<u8, DeviceControlOutput>,
}

impl App {
  pub fn create_device_control(
    &mut self,
    device_index: usize,
  ) -> anyhow::Result<DeviceControlState> {
    let device = match self.devices.get(device_index) {
      Some(d) => d,
      None => {
        log::warn!("goto_device_control: invalid device index {}", device_index);
        return Err(anyhow::anyhow!("Invalid device index"));
      }
    };

    let mut outputs = LiteMap::new();
    // TODO: support multiple output features per device by mapping slider index → feature index, and adding a slider for each output feature. For now just take the first vibrate output feature we find.
    for (index, feature) in device.outputs(OutputType::Vibrate).iter().enumerate() {
      let output = feature
        .feature()
        .output()
        .as_ref()
        .unwrap()
        .get(OutputType::Vibrate)
        .unwrap();
      outputs.insert(
        index as u8,
        DeviceControlOutput {
          feature: feature.clone(),
          step_count: output.step_count(),
          step_limit: output.step_limit().clone(),
          value: 0,
          step: 0,
        },
      );
    }

    Ok(DeviceControlState {
      device_index,
      // battery: None,
      // rssi: None,
      outputs,
    })
  }

  pub async fn on_device_control_navigation(
    &mut self,
    _state: &mut DeviceControlState,
    nav_event: &NavigationEvent,
  ) {
    match nav_event {
      NavigationEvent::Up | NavigationEvent::Down => {}
      NavigationEvent::Select => {
        self.goto_device_list();
      }
    }
  }

  /// Called when a slider value changes while in device control state.
  /// Maps slider `slider_index` → output feature index and sends the scaled command.
  pub async fn on_device_control_slider(
    &mut self,
    state: &mut DeviceControlState,
    slider_event: &SliderEvent,
  ) {
    let device = self.devices.get(state.device_index).unwrap();

    match slider_event {
      SliderEvent::Changed(slider_index, slider_value) => {
        if let Some(output) = state.outputs.get_mut(slider_index) {
          output.value = *slider_value;
          output.step = scale_slider_to_step(
            *slider_value,
            output.step_count,
            output.step_limit.start(),
            output.step_limit.end(),
          );
          log::info!(
            "Slider {} changed: value={}, step_count={}, step_limit={}..={}, step={}",
            slider_index,
            output.value,
            output.step_count,
            output.step_limit.start(),
            output.step_limit.end(),
            output.step
          );
          let cmd =
            ClientDeviceOutputCommand::Vibrate(ClientDeviceCommandValue::Steps(output.step));
          match output.feature.run_output(&cmd).await {
            Ok(_) => log::info!("Sent command for slider {}", slider_index),
            Err(e) => log::error!("Error sending command for slider {}: {:?}", slider_index, e),
          }
        }
      }
    }
  }

  /// Periodic tick: poll battery and RSSI from the current device.
  pub async fn on_device_control_tick(&mut self, state: &mut DeviceControlState) {
    // TODO
  }

  // ── Drawing ──────────────────────────────────────────────────────────────

  pub fn draw_device_control(&mut self, state: &DeviceControlState) -> anyhow::Result<()> {
    let Some(device) = self.devices.get(state.device_index) else {
      return Ok(());
    };

    let name = device.name().clone();

    Ok(())
  }
}

fn scale_slider_to_step(slider_value: u16, step_count: u32, step_min: i32, step_max: i32) -> i32 {
  let step_range = (step_max - step_min) as f64;
  let scaled_value =
    (slider_value as f64 / hw::slider::MAX_VALUE as f64) * step_range + (step_min as f64);
  scaled_value.round() as i32
}
