use buttplug_client::device::{
  ClientDeviceCommandValue,
  ClientDeviceFeature,
  ClientDeviceOutputCommand,
};
use buttplug_core::{
  message::OutputType,
  util::range::RangeInclusive,
};
use embedded_graphics::prelude::Point;
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
  ) -> anyhow::Result<DeviceControlState> {
    let device = match self.current_device() {
      Some(d) => d,
      None => {
        log::warn!("goto_device_control: no current device");
        return Err(anyhow::anyhow!("No current device"));
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
        // Stop current output and ask the server device manager to disconnect the device.
        if let Some(current_index) = self.current_device_index {
          if let Some(device) = self.devices.get_mut(current_index) {
            if let Err(e) = device.disconnect().await {
              log::warn!("Error disconnecting device on back-navigate: {:?}", e);
            }
          }
        }
        self.current_device_index = None;
        self.goto_device_list();
        self.ensure_device_list_scanning().await;
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
          self.queue_draw();
        }
      }
    }
  }

  /// Periodic tick: poll battery and RSSI from the current device.
  pub async fn on_device_control_tick(&mut self, _state: &mut DeviceControlState) {
    // TODO
  }

  // ── Drawing ──────────────────────────────────────────────────────────────

  pub fn draw_device_control(&mut self, state: &DeviceControlState) -> anyhow::Result<()> {
    let Some(current_index) = self.current_device_index else {
      return Ok(());
    };

    let Some(app_device) = self.devices.get(current_index) else {
      return Ok(());
    };

    let screen = self.display.get_mut_canvas();

    utils::draw::draw_text(
      screen,
      &MAIN_FONT,
      &format!("Name: {}", app_device.name()),
      Point::new(0, 0),
    )?;

    let address = app_device.address().unwrap_or("-");
    utils::draw::draw_text(
      screen,
      &SMALL_FONT,
      &format!("Addr: {}", address),
      Point::new(0, 12),
    )?;

    let pretty_name = app_device.pretty_name().unwrap_or("-");
    utils::draw::draw_text(
      screen,
      &SMALL_FONT,
      &format!("Pretty: {}", pretty_name),
      Point::new(0, 20),
    )?;

    for (line, (_, output)) in state.outputs.iter().take(2).enumerate() {
      utils::draw::draw_text(
        screen,
        &SMALL_FONT,
        &format!("S{}: {} ({})", line + 1, output.value, output.step),
        Point::new(0, 32 + (line as i32 * 8)),
      )?;
    }

    Ok(())
  }
}

fn scale_slider_to_step(
  slider_value: u16,
  _step_count: u32,
  step_min: i32,
  step_max: i32,
) -> i32 {
  let step_range = (step_max - step_min) as f64;
  let scaled_value =
    (slider_value as f64 / hw::SLIDER_MAX_VALUE as f64) * step_range + (step_min as f64);
  scaled_value.round() as i32
}
