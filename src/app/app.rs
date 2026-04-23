use std::{
  cell::{Cell, RefCell, RefMut},
  pin::Pin,
  sync::Arc,
};

use buttplug_client::ButtplugClientEvent;
use buttplug_core::errors::ButtplugError;
use futures::{Stream, stream::StreamExt};
use futures_concurrency::prelude::*;
use tokio::sync::broadcast;

use crate::{
  app::{
    AppDevice,
    AppEvent,
    NavigationEvent,
    SliderEvent,
    app_device_control::DeviceControlState,
    app_device_list::DeviceListState,
  },
  buttplug::{
    self,
    backdoor::{ButtplugBackdoorEvent, DiscoveredDevice},
  },
  hw,
  utils,
};

#[derive(Debug)]
pub enum AppState {
  Idle,
  Connecting,
  Disconnecting,
  Error(String),
  DeviceList(DeviceListState),
  DeviceControl(DeviceControlState),
}

pub struct App {
  display: RefCell<hw::Display>,
  #[cfg(target_os = "espidf")]
  adc: hw::AdcInputs,
  server: Option<Arc<buttplug_server::ButtplugServer>>,
  client: buttplug_client::ButtplugClient,
  loading: bool,
  scanning: bool,
  state: Option<AppState>,
  devices: Vec<AppDevice>,
  current_device_index: Option<usize>,
  /// Controller's own battery level (0–100). Updated on every Tick from ADC.
  self_battery: u8,
  /// Last raw ADC value used to compute self_battery. Used for hysteresis.
  #[cfg(target_os = "espidf")]
  battery_raw_last: u16,
  tx: broadcast::Sender<AppEvent>,
  draw_pending: Cell<bool>,
}

impl App {
  pub fn new(display: hw::Display, #[cfg(target_os = "espidf")] adc: hw::AdcInputs) -> Self {
    let (tx, _) = broadcast::channel(64);

    App {
      display: RefCell::new(display),
      #[cfg(target_os = "espidf")]
      adc,
      server: None,
      client: buttplug_client::ButtplugClient::new("esp"),
      loading: true,
      scanning: false,
      state: Some(AppState::Idle),
      devices: Vec::new(),
      current_device_index: None,
      self_battery: 0,
      #[cfg(target_os = "espidf")]
      battery_raw_last: 0,
      tx,
      draw_pending: Cell::new(false),
    }
  }

  pub async fn main(
    mut self,
    input_event_stream: Pin<Box<dyn Stream<Item = AppEvent> + Send>>,
  ) -> anyhow::Result<()> {
    // Draw immediately so stale pixels from previous runs are cleared while
    // buttplug initialization and data loading are still in flight.
    self.draw().await?;

    let (server, connector, backdoor_stream) = buttplug::create_buttplug()?;
    self.server = Some(server.clone());

    let client_stream = self
      .client
      .event_stream()
      .map(|event| AppEvent::ButtplugEvent(event));

    let self_stream = utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe());

    let mut event_stream = Box::pin(
      (
        client_stream,
        self_stream,
        input_event_stream,
        backdoor_stream.map(AppEvent::BackdoorEvent),
      )
        .merge(),
    );

    self.client.connect(connector).await?;

    self.loading = false;
    self.queue_draw();

    while let Some(event) = event_stream.next().await {
      log::info!(target: "esp:butt::app::event", "{event}");
      match event {
        AppEvent::ButtplugEvent(ref event) => self.on_buttplug_event(event).await,
        AppEvent::Navigation(ref nav_event) => self.on_navigation(nav_event).await,
        AppEvent::Slider(ref slider_event) => self.on_slider(slider_event).await,
        AppEvent::Draw => self.on_draw().await,
        AppEvent::Tick => self.on_tick().await,
        AppEvent::Quit => {
          log::info!("Quit event received, exiting");
          return Ok(());
        }
        AppEvent::BackdoorEvent(event) => self.on_backdoor_event(event).await,
      }
    }

    Ok(())
  }

  pub(super) fn loading(&self) -> bool {
    self.loading
  }

  pub(super) fn battery(&self) -> u8 {
    self.self_battery
  }

  pub(super) fn screen(&self) -> RefMut<'_, hw::DisplayCanvas> {
    RefMut::map(self.display.borrow_mut(), |d| d.get_mut_canvas())
  }

  pub(super) fn devices(&self) -> &[AppDevice] {
    &self.devices
  }

  pub(super) fn current_device(&self) -> Option<&AppDevice> {
    self
      .current_device_index
      .and_then(|index| self.devices.get(index))
  }

  pub(super) fn current_device_mut(&mut self) -> Option<&mut AppDevice> {
    self
      .current_device_index
      .and_then(move |index| self.devices.get_mut(index))
  }

  fn add_device(&mut self, device: AppDevice) {
    if let Some(existing) = self
      .devices
      .iter_mut()
      .find(|d| d.address() == device.address())
    {
      *existing = device;
    } else {
      self.devices.push(device);
    }
  }

  pub(super) async fn connect_device(&mut self, device_index: usize) {
    if self.scanning {
      log::info!("Stopping scan before connecting to device");
      match self.client.stop_scanning().await {
        Ok(_) => {
          self.scanning = false;
        }
        Err(e) => {
          log::error!("Error stopping scan before connect: {:?}", e);
          return;
        }
      }
    }

    if let Err(e) = self.devices[device_index].connect().await {
      log::warn!("Error connecting selected device: {:?}", e);
      self.queue_draw();
    } else {
      self.current_device_index = Some(device_index);
      self.goto_connecting();
    }
  }

  pub(super) async fn disconnect_current_device(&mut self) {
    self.goto_disconnecting();
    // Draw immediately: the event loop is about to be blocked for the entire
    // BLE disconnect, so a queued Draw will never be processed in time.
    if let Err(e) = self.draw().await {
      log::warn!("Error drawing disconnecting screen: {:?}", e);
    }
    if let Some(device) = self.current_device_mut() {
      if let Err(e) = device.disconnect().await {
        log::warn!("Error disconnecting device: {:?}", e);
        self.current_device_index = None;
        self.goto_device_list().await;
      }
      log::info!("Disconnected from device");
      // On success, DeviceRemoved event will transition to device list
    } else {
      log::error!("disconnect_current called but no current device is set");
      self.goto_device_list().await;
    }
  }

  fn send(&self, event: AppEvent) {
    if let Err(e) = self.tx.send(event) {
      log::error!("Error sending event: {:?}", e);
    }
  }

  fn set_state(&mut self, new_state: AppState) {
    log::debug!("Transitioning to state: {:?}", new_state);
    self.state = Some(new_state);
    self.queue_draw();
  }

  pub(super) async fn goto_idle(&mut self) {
    self.set_state(AppState::Idle);
     if self.scanning {
      log::info!("Stopping scan before going idle");
      match self.client.stop_scanning().await {
        Ok(_) => {
          self.scanning = false;
        }
        Err(e) => {
          log::error!("Error stopping scan before going idle: {:?}", e);
          return;
        }
      }
    }
  }

  pub(super) async fn goto_device_list(&mut self) {
    self.set_state(AppState::DeviceList(DeviceListState::default()));
    self.ensure_scanning().await;
  }

  pub(super) fn goto_device_control(&mut self) {
    match self.current_device() {
      Some(d) => self.set_state(AppState::DeviceControl(DeviceControlState::new(d))),
      None => {
        log::error!("goto_device_control: no current device");
      }
    };
  }

  pub(super) fn goto_disconnecting(&mut self) {
    self.set_state(AppState::Disconnecting);
  }

  pub(super) fn goto_connecting(&mut self) {
    self.set_state(AppState::Connecting);
  }

  pub(super) fn goto_error(&mut self, message: String) {
    // Reset is_connecting on the failed device so it can be retried.
    if let Some(device) = self.current_device_mut() {
      device.clear_connected_device();
    }
    self.set_state(AppState::Error(message));
  }

  pub(super) fn remove_current_device(&mut self) {
    if let Some(index) = self.current_device_index.take() {
      self.devices.remove(index);
    }
  }

  pub(super) async fn ensure_scanning(&mut self) {
    if self.scanning {
      return;
    }

    match self.client.start_scanning().await {
      Ok(_) => {
        log::info!("Started scanning");
        self.scanning = true;
        self.queue_draw();
      }
      Err(e) => log::error!("Error ensuring scan is active: {:?}", e),
    }
  }

  async fn on_backdoor_event(&mut self, event: ButtplugBackdoorEvent) {
    match event {
      ButtplugBackdoorEvent::DeviceDiscovered(device) => {
        self.on_device_discovered(device);
      }
    }
  }

  fn on_device_discovered(&mut self, device: DiscoveredDevice) {
    let Some(server) = self.server.as_ref() else {
      log::warn!("Received backdoor event before server was initialized");
      return;
    };

    let app_device = AppDevice::from_discovered(device, server);
    self.add_device(app_device);
    self.queue_draw();
  }

  async fn on_buttplug_event(&mut self, event: &ButtplugClientEvent) {
    match event {
      ButtplugClientEvent::DeviceAdded(device) => {
        log::info!("Device added: {}", device.name());
        let device = device.clone();
        let device_index = device.index();
        if let Some(index) = self.current_device_index {
          if self.devices[index].is_connecting() {
            self.devices[index].set_connected_device(device);
            self.goto_device_control();
          } else {
            log::error!(
              "Received DeviceAdded (index {}) but current device '{}' is not connecting",
              device_index,
              self.devices[index].name()
            );
          }
        } else {
          log::error!(
            "Received DeviceAdded (index {}) but no current device is set",
            device_index
          );
        }
      }
      ButtplugClientEvent::DeviceRemoved(device) => {
        log::info!("Device removed: {}", device.name());
        if let Some(removed_index) = self
          .devices
          .iter()
          .position(|d| d.buttplug_index() == Some(device.index()))
        {
          self.devices[removed_index].clear_connected_device();

          match self.current_device_index {
            Some(current_index) if current_index == removed_index => {
              self.current_device_index = None;

              if matches!(
                self.state,
                Some(AppState::DeviceControl(_)) | Some(AppState::Disconnecting { .. })
              ) {
                log::info!("Current device removed, returning to device list");
                self.goto_device_list().await;
              }
            }
            _ => {}
          }
        }
      }
      ButtplugClientEvent::ServerDisconnect => {
        log::error!("Buttplug server disconnected");
        self.send(AppEvent::Quit);
      }
      ButtplugClientEvent::ScanningFinished => {
        log::info!("Buttplug scanning finished");
        self.scanning = false;
        self.queue_draw();
      }
      ButtplugClientEvent::Error(e) => {
        log::error!("Buttplug client error: {:?}", e);
        if matches!(self.state, Some(AppState::Connecting)) {
          let text = match e {
            ButtplugError::ButtplugDeviceError(e) => format!("ButtplugDeviceError\n{e:?}"),
            ButtplugError::ButtplugHandshakeError(e) => format!("ButtplugHandshakeError\n{e:?}"),
            ButtplugError::ButtplugMessageError(e) => format!("ButtplugMessageError\n{e:?}"),
            ButtplugError::ButtplugPingError(e) => format!("ButtplugPingError\n{e:?}"),
            ButtplugError::ButtplugUnknownError(e) => format!("ButtplugUnknownError\n{e:?}"),
          };
          self.goto_error(text);
        }
      }
      _ => {}
    }
  }

  async fn on_navigation(&mut self, nav_event: &NavigationEvent) {
    match self.state.take() {
      None => unreachable!("state is None during on_navigation"),
      Some(AppState::Idle) => {
        self.state = Some(AppState::Idle);
        self.on_idle_navigation(nav_event).await;
      }
      Some(AppState::DeviceList(mut state)) => {
        self.on_device_list_navigation(&mut state, nav_event).await;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceList(state));
        }
      }
      Some(AppState::DeviceControl(mut state)) => {
        self
          .on_device_control_navigation(&mut state, nav_event)
          .await;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceControl(state));
        }
      }
      Some(AppState::Error(state)) => {
        self.state = Some(AppState::Error(state));
        self.on_error_navigation(nav_event).await;
      }
      // otherwise just restore the state
      state => self.state = state,
    }
  }

  async fn on_slider(&mut self, slider_event: &SliderEvent) {
    match self.state.take() {
      Some(AppState::DeviceControl(mut state)) => {
        self
          .on_device_control_slider(&mut state, slider_event)
          .await;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceControl(state));
        }
      }
      // otherwise just restore the state
      state => self.state = state,
    }
  }

  async fn on_tick(&mut self) {
    match self.state.take() {
      Some(AppState::DeviceList(mut state)) => {
        self.on_device_list_tick(&mut state).await;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceList(state));
        }
      }
      // otherwise just restore the state
      state => self.state = state,
    }

    let mut any_changed = false;
    for device in self.devices.iter_mut() {
      if device.tick().await.unwrap_or(false) {
        any_changed = true;
      }
    }

    #[cfg(target_os = "espidf")]
    {
      // Hysteresis: only recompute when raw ADC moves by more than the threshold.
      // This prevents oscillation at percentage boundaries due to ADC noise.
      const BATTERY_HYSTERESIS: u16 = 8;
      let raw = self.adc.battery_raw();
      if raw.abs_diff(self.battery_raw_last) > BATTERY_HYSTERESIS {
        self.battery_raw_last = raw;
        log::info!("Battery raw ADC value: {}", raw);
        // Two-point calibration: raw 1815 → 3080 mV, raw 2420 → 4180 mV
        // slope = 20/11 mV/count, offset = -220 mV
        let v_bat_mv = (raw as u32 * 20 / 11).saturating_sub(220);
        // LiPo: 3000 mV (0%) – 4200 mV (100%)
        let pct = ((v_bat_mv.saturating_sub(3000)) * 100 / 1200).min(100) as u8;
        log::debug!("Battery: raw={} v_bat={}mV pct={}%", raw, v_bat_mv, pct);
        if pct != self.self_battery {
          self.self_battery = pct;
          any_changed = true;
        }
      }
    }

    if any_changed {
      self.queue_draw();
    }
  }

  pub(super) fn queue_draw(&self) {
    if !self.draw_pending.replace(true) {
      self.send(AppEvent::Draw);
    }
  }

  async fn on_draw(&mut self) {
    self.draw_pending.set(false);
    if let Err(e) = self.draw().await {
      log::error!("Error drawing: {:?}", e);
    }
  }

  async fn draw(&mut self) -> anyhow::Result<()> {
    self.screen().get_mut_buffer().fill(0);

    match self.state.take() {
      None => unreachable!("state is None during draw"),
      Some(AppState::Idle) => {
        self.state = Some(AppState::Idle);
        self.draw_idle()?;
      }
      Some(AppState::DeviceList(state)) => {
        self.draw_device_list(&state)?;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceList(state));
        }
      }
      Some(AppState::DeviceControl(state)) => {
        self.draw_device_control(&state)?;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceControl(state));
        }
      }
      Some(AppState::Connecting) => {
        self.state = Some(AppState::Connecting);
        self.draw_connecting()?;
      }
      Some(AppState::Disconnecting) => {
        self.state = Some(AppState::Disconnecting);
        self.draw_disconnecting()?;
      }
      Some(AppState::Error(message)) => {
        self.draw_error(&message)?;
        self.state = Some(AppState::Error(message));
      }
    }

    self.display.borrow_mut().flush_all()?;

    Ok(())
  }
}
