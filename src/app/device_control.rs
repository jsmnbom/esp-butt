use buttplug_client::device::{
  ClientDeviceCommandValue,
  ClientDeviceFeature,
  ClientDeviceOutputCommand,
};
use buttplug_core::{message::OutputType, util::range::RangeInclusive};
use futures_concurrency::future::Race;
use litemap::LiteMap;

use crate::{
  app::{App, AppDevice, MAIN_FONT, NavigationEvent, SMALL_FONT, SliderEvent},
  hw,
  img,
  utils::draw::*,
};

pub struct DeviceControlOutput {
  feature: ClientDeviceFeature,
  output_type: OutputType,
  step_count: u32,
  step_limit: RangeInclusive<i32>,
  value: u16,
  step: i32,
  last_sent_step: Option<i32>,
}

impl std::fmt::Debug for DeviceControlOutput {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("DeviceControlOutput")
      .field("output_type", &self.output_type)
      .field("step_count", &self.step_count)
      .field("step_limit", &self.step_limit)
      .field("value", &self.value)
      .field("step", &self.step)
      .finish()
  }
}

// The output types we support controlling from the device control screen, but also the priority we grab them in, in case a feature supports multiple output types.
// Whether any devices are supported set up in such a way i do not know :P
const SUPPORTED_OUTPUT_TYPES: [OutputType; 8] = [
  OutputType::Vibrate,
  OutputType::Rotate,
  OutputType::Oscillate,
  OutputType::Constrict,
  OutputType::Temperature,
  OutputType::Led,
  OutputType::Position,
  OutputType::Spray,
];

#[derive(Debug)]
pub struct DeviceControlState {
  /// Slider index to output feature index mapping
  pub outputs: LiteMap<u8, DeviceControlOutput>,
}

impl DeviceControlState {
  pub fn new(app_device: &AppDevice) -> Self {
    let device = app_device
      .client_device()
      .expect("DeviceControlState::new called without a client device");

    let mut outputs = LiteMap::new();

    let mut output_index = 0;

    for feature in device.device_features().values() {
      for output_type in SUPPORTED_OUTPUT_TYPES {
        if let Some(limits) = feature.feature().get_output_limits(output_type) {
          outputs.insert(
            output_index,
            DeviceControlOutput {
              feature: feature.clone(),
              output_type,
              step_count: limits.step_count(),
              step_limit: limits.step_limit().clone(),
              value: 0,
              step: 0,
              last_sent_step: None,
            },
          );
          output_index += 1;
          break; // Only grab the highest priority output type for each feature
        }
      }
    }
    Self { outputs }
  }
}

impl App {
  pub async fn on_device_control_navigation(
    &mut self,
    _state: &mut DeviceControlState,
    nav_event: &NavigationEvent,
  ) {
    match nav_event {
      NavigationEvent::Up | NavigationEvent::Down => {}
      NavigationEvent::Select => {
        if let Some(current_index) = self.current_device_index {
          self.goto_disconnecting();
          if let Some(device) = self.devices.get_mut(current_index) {
            if let Err(e) = device.disconnect().await {
              log::warn!("Error disconnecting device: {:?}", e);
              self.current_device_index = None;
              self.goto_device_list().await;
            }
            // On success, DeviceRemoved event will transition to device list
          }
        } else {
          self.goto_device_list().await;
        }
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
          if output.last_sent_step == Some(output.step) {
            self.queue_draw();
            return;
          }
          let cmd = ClientDeviceOutputCommand::from_command_value(
            output.output_type,
            &ClientDeviceCommandValue::Steps(output.step),
          )
          .unwrap();
          log::debug!(target: "slider", "sending command for slider {}", slider_index);
          let send_result = (
            async { Some(output.feature.run_output(&cmd).await) },
            async {
              crate::utils::task::sleep_timer_async(core::time::Duration::from_millis(500)).await;
              None
            },
          )
            .race()
            .await;
          log::debug!(target: "slider", "command finished for slider {}", slider_index);
          match send_result {
            Some(Ok(_)) => {
              output.last_sent_step = Some(output.step);
              log::info!("Sent command for slider {}", slider_index);
            }
            Some(Err(e)) => {
              log::error!("Error sending command for slider {}: {:?}", slider_index, e)
            }
            None => log::warn!(
              "Timeout sending command for slider {}, main loop unblocked",
              slider_index
            ),
          }
          self.queue_draw();
        }
      }
    }
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

    ControllerBattery {
      point: Point::new(0, 0),
      level: self.self_battery,
    }
    .draw(screen)?;

    Text::new(app_device.name(), Point::new(0, 6), &MAIN_FONT).draw(screen)?;
    Text::new(app_device.address(), Point::new(0, 16), &SMALL_FONT).draw(screen)?;
    if let Some(pretty) = app_device.pretty_name() {
      Text::new(pretty, Point::new(0, 24), &SMALL_FONT).draw(screen)?;
    }

    if let Some(rssi) = app_device.rssi() {
      SignalStrengthBar {
        point: Point::new(10, 40),
        rssi,
      }
      .draw(screen)?;
    }
    if let Some(battery) = app_device.battery() {
      DeviceBattery {
        point: Point::new(40, 46),
        level: battery as u8,
      }
      .draw(screen)?;
    }

    let start_x = if state.outputs.len() > 1 { 84 } else { 84 + 24 };

    for (i, (_, output)) in state.outputs.iter().take(2).enumerate() {
      let x = start_x + (i as i32 * 24);

      OutputSlider::new(x, output).draw(screen)?;
    }

    Ok(())
  }

  pub fn draw_disconnecting(&mut self) -> anyhow::Result<()> {
    let name = self
      .current_device()
      .map(|d| d.name().to_string())
      .unwrap_or_else(|| "?".to_string());
    let screen = self.display.get_mut_canvas();
    Text::new(&name, Point::new(0, 20), &MAIN_FONT).draw(screen)?;
    Text::new("Disconnecting...", Point::new(0, 36), &SMALL_FONT).draw(screen)?;
    Ok(())
  }
}

fn icon_for_output_type(output_type: OutputType) -> &'static ImageRaw<'static, BinaryColor> {
  match output_type {
    OutputType::Vibrate => &img::ICON_VIBRATE,
    OutputType::Rotate => &img::ICON_ROTATE,
    OutputType::Oscillate => &img::ICON_OSCILLATE,
    OutputType::Constrict => &img::ICON_CONSTRICT,
    OutputType::Temperature => &img::ICON_TEMPERATURE,
    OutputType::Led => &img::ICON_LED,
    OutputType::Position => &img::ICON_POSITION,
    OutputType::HwPositionWithDuration => &img::ICON_HW_POSITION_WITH_DURATION,
    OutputType::Spray => &img::ICON_SPRAY,
  }
}

fn scale_slider_to_step(slider_value: u16, _step_count: u32, step_min: i32, step_max: i32) -> i32 {
  let step_range = (step_max - step_min) as f64;
  let scaled_value =
    (slider_value as f64 / hw::SLIDER_MAX_VALUE as f64) * step_range + (step_min as f64);
  scaled_value.round() as i32
}

struct OutputSlider<'a> {
  x: i32,
  output: &'a DeviceControlOutput,
}

impl OutputSlider<'_> {
  fn new<'a>(x: i32, output: &'a DeviceControlOutput) -> OutputSlider<'a> {
    OutputSlider { x, output }
  }
}

impl<'a> Drawable for OutputSlider<'a> {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    RoundedRectangle::new(
      Rectangle::new(Point::new(self.x + 4, 51), Size::new(13, 13)),
      CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;
    Image::new(
      icon_for_output_type(self.output.output_type),
      Point::new(self.x + 6, 53),
    )
    .draw(target)?;

    RoundedRectangle::new(
      Rectangle::new(Point::new(self.x + 13, 0), Size::new(4, 50)),
      CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;

    Text::new("0", Point::new(self.x + 7, 44), &SMALL_FONT)
      .align(HorizontalAlignment::Right)
      .draw(target)?;

    Line::new(Point::new(self.x + 8, 47), Point::new(self.x + 10, 47))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;

    Text::new(
      &format!("{}", self.output.step_count),
      Point::new(self.x + 7, 0),
      &SMALL_FONT,
    )
    .align(HorizontalAlignment::Right)
    .draw(target)?;

    Line::new(Point::new(self.x + 8, 3), Point::new(self.x + 10, 3))
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;

    RoundedRectangle::new(
      Rectangle::new(Point::new(self.x, 22), Size::new(11, 9)),
      CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;

    Text::new(
      &format!("{}", self.output.step),
      Point::new(self.x + 9, 23),
      &SMALL_FONT,
    )
    .align(HorizontalAlignment::Right)
    .draw(target)?;

    Ok(())
  }
}
