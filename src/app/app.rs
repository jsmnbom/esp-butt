use std::{pin::Pin, sync::Arc};

use buttplug_client::ButtplugClientDevice;
use buttplug_client::ButtplugClientEvent;
use futures::{Stream, stream::StreamExt};
use futures_concurrency::prelude::*;
use tokio::sync::broadcast;

use crate::{
  app::{
    AppEvent,
    NavigationEvent,
    SliderEvent,
    device_control::DeviceControlState,
    device_list::DeviceListState,
  },
  buttplug::{self, deferred::DiscoveredDevice},
  hw,
  utils,
};

#[derive(Debug)]
pub enum AppState {
  Idle,
  DeviceList(DeviceListState),
  DeviceControl(DeviceControlState),
}

pub struct App {
  pub(super) display: hw::Display,
  pub(super) server: Option<Arc<buttplug_server::ButtplugServer>>,
  pub(super) client: buttplug_client::ButtplugClient,

  pub(super) scanning: bool,
  pub(super) state: Option<AppState>,

  /// Protocol-matched devices discovered during scanning, shown in the device list.
  pub(super) devices: Vec<DiscoveredDevice>,
  /// Name of the device with an in-flight connection attempt.
  pub(super) pending_connect: Option<String>,
  /// Full list of devices that Buttplug has reported as connected.
  pub(super) connected_devices: Vec<ButtplugClientDevice>,
  /// Index into `connected_devices` for the device currently being controlled.
  pub(super) current_device_index: Option<usize>,

  tx: broadcast::Sender<AppEvent>,
  input_event_stream: Option<Pin<Box<dyn Stream<Item = AppEvent> + Send>>>,
}

impl App {
  pub fn new(
    display: hw::Display,
    input_event_stream: Pin<Box<dyn Stream<Item = AppEvent> + Send>>,
  ) -> Self {
    let (tx, _) = broadcast::channel(16);

    App {
      display,
      server: None,
      client: buttplug_client::ButtplugClient::new("esp"),

      scanning: false,
      state: Some(AppState::Idle),

      devices: Vec::new(),
      pending_connect: None,
      connected_devices: Vec::new(),
      current_device_index: None,

      tx,
      input_event_stream: Some(input_event_stream),
    }
  }

  pub async fn main(mut self) -> anyhow::Result<()> {
    let (server, connector, discovery_stream) = buttplug::create_buttplug()?;
    self.server = Some(server.clone());

    let client_stream = self
      .client
      .event_stream()
      .map(|event| AppEvent::ButtplugEvent(event));

    let discovery_stream = discovery_stream.map(|device| AppEvent::DeviceDiscovered(device));
    let self_stream = utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe());

    let mut event_stream = Box::pin(
      (
        client_stream,
        self_stream,
        self.input_event_stream.take().unwrap(),
        discovery_stream,
      )
        .merge(),
    );

    self.client.connect(connector).await?;

    self.queue_draw();

    while let Some(event) = event_stream.next().await {
      log::info!(target: "app_events", "{event:?}");
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
        AppEvent::DeviceDiscovered(device) => self.on_device_approval_requested(device),
      }
    }

    Ok(())
  }

  pub(super) fn send(&self, event: AppEvent) {
    if let Err(e) = self.tx.send(event) {
      log::error!("Error sending event: {:?}", e);
    }
  }

  pub(super) fn goto_idle(&mut self) {
    self.set_state(AppState::Idle);
  }

  pub(super) fn goto_device_list(&mut self) {
    self.set_state(AppState::DeviceList(DeviceListState { cursor: 0 }));
  }

  pub(super) fn goto_device_control(&mut self) {
    match self.create_device_control() {
      Ok(state) => self.set_state(AppState::DeviceControl(state)),
      Err(e) => log::error!("Error creating device control state: {:?}", e),
    }
  }

  pub(super) fn current_device(&self) -> Option<&ButtplugClientDevice> {
    self
      .current_device_index
      .and_then(|index| self.connected_devices.get(index))
  }

  fn set_state(&mut self, new_state: AppState) {
    self.state = Some(new_state);
    self.queue_draw();
  }

  fn on_device_approval_requested(&mut self, device: DiscoveredDevice) {
    // Replace existing entry with same name (e.g. re-discovered after re-scan) or append.
    if let Some(existing) = self.devices.iter_mut().find(|d| d.name == device.name) {
      *existing = device;
    } else {
      self.devices.push(device);
    }
    self.queue_draw();
  }

  async fn on_buttplug_event(&mut self, event: &ButtplugClientEvent) {
    match event {
      ButtplugClientEvent::DeviceAdded(device) => {
        log::info!("Device added: {}", device.name());
        let device = device.clone();
        let connected_index = if let Some(index) = self
          .connected_devices
          .iter()
          .position(|connected| connected.index() == device.index())
        {
          self.connected_devices[index] = device;
          index
        } else {
          self.connected_devices.push(device);
          self.connected_devices.len() - 1
        };

        if self.pending_connect.take().is_some() {
          self.current_device_index = Some(connected_index);
          self.goto_device_control();
        }
      }
      ButtplugClientEvent::DeviceRemoved(device) => {
        log::info!("Device removed: {}", device.name());
        if let Some(removed_index) = self
          .connected_devices
          .iter()
          .position(|connected| connected.index() == device.index())
        {
          self.connected_devices.remove(removed_index);

          match self.current_device_index {
            Some(current_index) if current_index == removed_index => {
              self.current_device_index = None;

              if let Some(AppState::DeviceControl(_)) = self.state {
                log::info!("Current device removed, returning to device list");
                self.goto_device_list();
              }
            }
            Some(current_index) if current_index > removed_index => {
              self.current_device_index = Some(current_index - 1);
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
        self.on_idle_navigation(nav_event);
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
      Some(AppState::DeviceControl(mut state)) => {
        self.on_device_control_tick(&mut state).await;
        if self.state.is_none() {
          self.state = Some(AppState::DeviceControl(state));
        }
      }
      // otherwise just restore the state
      state => self.state = state,
    }
  }

  pub(super) fn queue_draw(&self) {
    self.send(AppEvent::Draw);
  }

  async fn on_draw(&mut self) {
    //log::info!("Drawing screen with state: {:?}", self.state);
    if let Err(e) = self.draw().await {
      log::error!("Error drawing: {:?}", e);
    }
  }

  async fn draw(&mut self) -> anyhow::Result<()> {
    self.display.get_mut_canvas().get_mut_buffer().fill(0);

    match self.state.take() {
      None => unreachable!("state is None during on_navigation"),
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
    }

    self.display.flush_all()?;

    Ok(())
  }
}
