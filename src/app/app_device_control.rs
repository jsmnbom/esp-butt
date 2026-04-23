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
  hw::{self, DisplayCanvas},
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
        self.disconnect_current_device().await;
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
          log::debug!(
            "Slider {} changed, sending command: {:?}",
            slider_index,
            cmd
          );
          let send_result = (
            async { Some(output.feature.run_output(&cmd).await) },
            async {
              crate::utils::task::sleep_timer_async(core::time::Duration::from_millis(500)).await;
              None
            },
          )
            .race()
            .await;
          match send_result {
            Some(Ok(_)) => {
              output.last_sent_step = Some(output.step);
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

  pub fn draw_current_device_text(
    &self,
    screen: &mut DisplayCanvas,
    last_line: Option<&str>,
  ) -> anyhow::Result<()> {
    Text::new(
      self.current_device().map(|d| d.name()).unwrap_or("???"),
      Point::new(64, 12),
      &MAIN_FONT,
    )
    .align(HorizontalAlignment::Center)
    .draw(screen)?;
    Text::new(
      self.current_device().map(|d| d.address()).unwrap_or("???"),
      Point::new(64, 22),
      &SMALL_FONT,
    )
    .align(HorizontalAlignment::Center)
    .draw(screen)?;
    if let Some(last) = last_line {
      Text::new(last, Point::new(64, 30), &SMALL_FONT)
        .align(HorizontalAlignment::Center)
        .draw(screen)?;
    }

    Ok(())
  }

  pub fn draw_device_control(&self, state: &DeviceControlState) -> anyhow::Result<()> {
    let Some(device) = self.current_device() else {
      return Ok(());
    };

    let mut screen = self.screen();
    let screen = &mut *screen;

    ControllerBattery {
      point: Point::new(60, 0),
      level: self.battery(),
    }
    .draw(screen)?;

    self.draw_current_device_text(screen, device.pretty_name())?;

    if let Some(rssi) = device.rssi() {
      SignalStrengthBar {
        point: Point::new(45, 40),
        rssi,
      }
      .draw(screen)?;
    }
    if let Some(battery) = device.battery() {
      DeviceBattery {
        point: Point::new(65, 46),
        level: battery.min(100) as u8,
      }
      .draw(screen)?;
    }

    OutputSlider::new(0, state.outputs.get(&0), true).draw(screen)?;
    OutputSlider::new(110, state.outputs.get(&1), false).draw(screen)?;

    Ok(())
  }

  pub fn draw_connecting(&self) -> anyhow::Result<()> {
    let mut screen = self.screen();
    let screen = &mut *screen;
    self.draw_current_device_text(screen, Some("Connecting..."))
  }

  pub fn draw_disconnecting(&self) -> anyhow::Result<()> {
    let mut screen = self.screen();
    let screen = &mut *screen;
    self.draw_current_device_text(screen, Some("Disconnecting..."))
  }
}

fn icon_for_output_type(
  output_type: Option<OutputType>,
) -> &'static ImageRaw<'static, BinaryColor> {
  match output_type {
    Some(OutputType::Vibrate) => &img::ICON_VIBRATE,
    Some(OutputType::Rotate) => &img::ICON_ROTATE,
    Some(OutputType::Oscillate) => &img::ICON_OSCILLATE,
    Some(OutputType::Constrict) => &img::ICON_CONSTRICT,
    Some(OutputType::Temperature) => &img::ICON_TEMPERATURE,
    Some(OutputType::Led) => &img::ICON_LED,
    Some(OutputType::Position) => &img::ICON_POSITION,
    Some(OutputType::HwPositionWithDuration) => &img::ICON_HW_POSITION_WITH_DURATION,
    Some(OutputType::Spray) => &img::ICON_SPRAY,
    None => &img::ICON_NONE,
  }
}

fn scale_slider_to_step(slider_value: u16, _step_count: u32, step_min: i32, step_max: i32) -> i32 {
  let step_range = step_max as f64 - step_min as f64;
  let scaled_value =
    (slider_value as f64 / hw::SLIDER_MAX_VALUE as f64) * step_range + (step_min as f64);
  scaled_value.round() as i32
}

struct OutputSlider<'a> {
  x: i32,
  output: Option<&'a DeviceControlOutput>,
  mirror: bool,
}

impl OutputSlider<'_> {
  fn new<'a>(x: i32, output: Option<&'a DeviceControlOutput>, mirror: bool) -> OutputSlider<'a> {
    OutputSlider { x, output, mirror }
  }
}

impl<'a> Drawable for OutputSlider<'a> {
  type Color = BinaryColor;
  type Output = ();

  fn draw<D>(&self, target: &mut D) -> Result<Self::Output, D::Error>
  where
    D: DrawTarget<Color = Self::Color>,
  {
    let text_alignment = if self.mirror {
      HorizontalAlignment::Left
    } else {
      HorizontalAlignment::Right
    };
    let bar_x = if self.mirror { self.x } else { self.x + 13 };
    let bar_line_x = if self.mirror { self.x + 2 } else { self.x + 15 };
    let type_rect_x = if self.mirror { self.x } else { self.x + 5 };
    let type_img_x = if self.mirror { self.x + 2 } else { self.x + 7 };
    let step_text_x = if self.mirror { self.x + 10 } else { self.x + 8 };
    let step_line_x = if self.mirror { self.x + 6 } else { self.x + 9 };
    let cur_step_rect_x = if self.mirror { self.x + 7 } else { self.x };
    let cur_step_text_x = if self.mirror { self.x + 9 } else { self.x + 9 };

    RoundedRectangle::new(
      Rectangle::new(Point::new(bar_x, 0), Size::new(5, 50)),
      CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;

    RoundedRectangle::new(
      Rectangle::new(Point::new(type_rect_x, 51), Size::new(13, 13)),
      CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
    )
    .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
    .draw(target)?;

    Image::new(
      icon_for_output_type(self.output.map(|o| o.output_type)),
      Point::new(type_img_x, 53),
    )
    .draw(target)?;

    if let Some(output) = self.output {
      let bar_line_y = 47 - (output.value as i32 * 45) / hw::SLIDER_MAX_VALUE as i32;
      if bar_line_y < 47 {
        Line::new(
          Point::new(bar_line_x, bar_line_y),
          Point::new(bar_line_x, 47),
        )
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;
      }

      Text::new("0", Point::new(step_text_x, 44), &SMALL_FONT)
        .align(text_alignment)
        .draw(target)?;

      Line::new(Point::new(step_line_x, 47), Point::new(step_line_x + 2, 47))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;

      Text::new(
        &format!("{}", output.step_count),
        Point::new(step_text_x, 0),
        &SMALL_FONT,
      )
      .align(text_alignment)
      .draw(target)?;

      Line::new(Point::new(step_line_x, 3), Point::new(step_line_x + 2, 3))
        .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
        .draw(target)?;

      RoundedRectangle::new(
        Rectangle::new(Point::new(cur_step_rect_x, 22), Size::new(11, 9)),
        CornerRadiiBuilder::new().all(Size::new(2, 2)).build(),
      )
      .into_styled(PrimitiveStyle::with_stroke(BinaryColor::On, 1))
      .draw(target)?;

      Text::new(
        &format!("{}", output.step),
        Point::new(cur_step_text_x, 23),
        &SMALL_FONT,
      )
      .align(text_alignment)
      .draw(target)?;
    }

    Ok(())
  }
}
