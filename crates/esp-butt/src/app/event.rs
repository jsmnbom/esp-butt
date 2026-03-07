use buttplug_client::ButtplugClientEvent;

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
  /// Signals that the display should be redrawn, e.g. because the UI state has changed and the display needs to reflect that
  Draw,
  /// Periodic tick for polling device state (battery, RSSI)
  Tick,
  Quit,
}
