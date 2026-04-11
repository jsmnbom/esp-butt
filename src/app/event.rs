use buttplug_client::ButtplugClientEvent;

use crate::buttplug::backdoor::ButtplugBackdoorEvent;

#[derive(Debug, Clone, Copy)]
pub enum NavigationEvent {
  /// Encoder turned one "click" in the counter-clockwise direction
  Up,
  /// Encoder turned one "click" in the clockwise direction
  Down,
  /// Encoder button pressed
  Select,
}

#[derive(Debug, Clone, Copy)]
pub enum SliderEvent {
  /// Slider with the given index changed to the given value (0–4095)
  Changed(u8, u16),
}

#[derive(Debug, Clone)]
pub enum AppEvent {
  /// Navigation events from the encoder
  Navigation(NavigationEvent),
  /// Slider change events, with the slider index and new value
  Slider(SliderEvent),
  /// Events from the Buttplug client, such as device connections/disconnections and messages from devices
  ButtplugEvent(ButtplugClientEvent),
  /// Events from the "backdoor" channel, such as discovered devices and RSSI updates
  BackdoorEvent(ButtplugBackdoorEvent),
  /// Signals that the display should be redrawn, e.g. because the UI state has changed and the display needs to reflect that
  Draw,
  /// Periodic tick for polling device state (battery, RSSI)
  Tick,
  Quit,
}

impl std::fmt::Display for AppEvent {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      AppEvent::Navigation(nav) => write!(f, "Navigation({nav:?})"),
      AppEvent::Slider(SliderEvent::Changed(index, value)) => {
        write!(f, "SliderChanged(index={index}, value={value})")
      }
      AppEvent::ButtplugEvent(event) => write!(f, "ButtplugEvent({event:?})"),
      AppEvent::BackdoorEvent(event) => write!(f, "BackdoorEvent({event})"),
      AppEvent::Draw => write!(f, "Draw"),
      AppEvent::Tick => write!(f, "Tick"),
      AppEvent::Quit => write!(f, "Quit"),
    }
  }
}
