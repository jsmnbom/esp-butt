use buttplug_client::ButtplugClientEvent;

#[derive(Debug, Clone)]
pub enum NavigationEvent {
  /// Encoder turned one "click" in the counter-clockwise direction
  Up,
  /// Encoder turned one "click" in the clockwise direction
  Down,
  /// Encoder button pressed
  Select,
}


#[derive(Debug, Clone)]
pub enum AppEvent {
  /// Navigation events from the encoder
  Navigation(NavigationEvent),
  /// Slider change events, with the slider index and new value
  SliderChanged(u8, u16),
  /// Events from the Buttplug client, such as device connections/disconnections and messages from devices
  ButtplugEvent(ButtplugClientEvent),
  /// Signals that the display should be redrawn, e.g. because the UI state has changed and the display needs to reflect that
  Draw,
  #[cfg(not(target_os = "espidf"))]
  Quit,
}
