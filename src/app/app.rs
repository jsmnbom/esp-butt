use std::{pin::Pin, sync::Arc};

use buttplug_client::ButtplugClientEvent;
use futures::{Stream, stream::StreamExt};
use futures_concurrency::prelude::*;
use tokio::sync::broadcast;

use crate::{
  app::{
    AppDevice,
    AppEvent,
    NavigationEvent,
    SliderEvent,
    device_control::DeviceControlState,
    device_list::DeviceListState,
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
  DeviceList(DeviceListState),
  DeviceControl(DeviceControlState),
}

pub struct App {
  pub(super) display: hw::Display,
  #[cfg(target_os = "espidf")]
  pub(super) adc: hw::AdcInputs,
  pub(super) server: Option<Arc<buttplug_server::ButtplugServer>>,
  pub(super) client: buttplug_client::ButtplugClient,
  pub(super) boot_loading: bool,

  pub(super) scanning: bool,
  pub(super) state: Option<AppState>,

  /// Unified app devices that can be discovered and/or connected.
  pub(super) devices: Vec<AppDevice>,
  /// Index into `devices` for the device currently being controlled.
  pub(super) current_device_index: Option<usize>,

  /// Controller's own battery level (0–100). Updated on every Tick from ADC.
  pub(super) self_battery: u8,

  tx: broadcast::Sender<AppEvent>,
}

impl App {
  pub fn new(
    display: hw::Display,
    #[cfg(target_os = "espidf")] adc: hw::AdcInputs,
  ) -> Self {
    let (tx, _) = broadcast::channel(64);

    App {
      display,
      #[cfg(target_os = "espidf")]
      adc,
      server: None,
      client: buttplug_client::ButtplugClient::new("esp"),
      boot_loading: true,

      scanning: false,
      state: Some(AppState::Idle),

      devices: Vec::new(),
      current_device_index: None,

      self_battery: 0,

      tx,
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
    self.boot_loading = false;

    self.queue_draw();

    while let Some(event) = event_stream.next().await {
      log::info!(target: "app_events", "{event}");
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

  pub(super) fn send(&self, event: AppEvent) {
    if let Err(e) = self.tx.send(event) {
      log::error!("Error sending event: {:?}", e);
    }
  }

  fn set_state(&mut self, new_state: AppState) {
    log::debug!("Transitioning to state: {:?}", new_state);
    self.state = Some(new_state);
    self.queue_draw();
  }

  pub(super) fn goto_idle(&mut self) {
    self.set_state(AppState::Idle);
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

  pub(super) fn current_device(&self) -> Option<&AppDevice> {
    self
      .current_device_index
      .and_then(|index| self.devices.get(index))
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
    if let Some(existing) = self
      .devices
      .iter_mut()
      .find(|d| d.address() == app_device.address())
    {
      *existing = app_device;
    } else {
      self.devices.push(app_device);
    }
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
      Some(state @ AppState::Connecting { .. }) | Some(state @ AppState::Disconnecting { .. }) => {
        self.state = Some(state);
      }
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
      let raw = self.adc.battery_raw();
      // V_BAT_mV ≈ raw * 6200 / 4095  (db12 full-scale ~3100 mV, ×2 voltage divider)
      let v_bat_mv = raw as u32 * 6200 / 4095;
      // LiPo: 3000 mV (0%) – 4200 mV (100%)
      let pct = ((v_bat_mv.saturating_sub(3000)) * 100 / 1200).min(100) as u8;
      if pct != self.self_battery {
        log::debug!("Battery: raw={} v_bat={}mV pct={}%", raw, v_bat_mv, pct);
        self.self_battery = pct;
        any_changed = true;
      }
    }

    if any_changed {
      self.queue_draw();
    }
  }

  pub(super) fn queue_draw(&self) {
    self.send(AppEvent::Draw);
  }

  async fn on_draw(&mut self) {
    if let Err(e) = self.draw().await {
      log::error!("Error drawing: {:?}", e);
    }
  }

  async fn draw(&mut self) -> anyhow::Result<()> {
    self.display.get_mut_canvas().get_mut_buffer().fill(0);

    match self.state.take() {
      None => unreachable!("state is None during draw"),
      Some(AppState::Idle) => {
        self.state = Some(AppState::Idle);
        self.draw_idle()?;
      }
      Some(AppState::DeviceList(mut state)) => {
        self.draw_device_list(&mut state)?;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceList(state));
        }
      }
      Some(AppState::DeviceControl(mut state)) => {
        self.draw_device_control(&mut state)?;
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
    }

    self.display.flush_all()?;

    Ok(())
  }
}
