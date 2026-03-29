use std::{pin::Pin, sync::Arc};

use buttplug_client::ButtplugClientDevice;
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
  pub(super) boot_loading: bool,

  pub(super) scanning: bool,
  pub(super) state: Option<AppState>,

  /// Unified app devices that can be discovered and/or connected.
  pub(super) devices: Vec<AppDevice>,
  /// Index of the device with an in-flight connection attempt.
  pub(super) pending_connect: Option<usize>,
  /// Index into `devices` for the device currently being controlled.
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
      boot_loading: true,

      scanning: false,
      state: Some(AppState::Idle),

      devices: Vec::new(),
      pending_connect: None,
      current_device_index: None,

      tx,
      input_event_stream: Some(input_event_stream),
    }
  }

  pub async fn main(mut self) -> anyhow::Result<()> {
    // Draw immediately so stale pixels from previous runs are cleared while
    // buttplug initialization and data loading are still in flight.
    self.draw().await?;

    buttplug::init();

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
    self.boot_loading = false;

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
        AppEvent::DeviceDiscovered(device) => self.on_device_discovered(device),
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
    self.set_state(AppState::DeviceList(DeviceListState {
      cursor: 0,
      last_activity: std::time::Instant::now(),
    }));
  }

  pub(super) async fn ensure_device_list_scanning(&mut self) {
    if !matches!(self.state, Some(AppState::DeviceList(_))) || self.scanning {
      return;
    }

    match self.client.start_scanning().await {
      Ok(_) => {
        self.scanning = true;
        self.queue_draw();
      }
      Err(e) => log::error!("Error ensuring scan is active on device list: {:?}", e),
    }
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
      .and_then(|index| self.devices.get(index))
      .and_then(|device| device.client_device())
  }

  fn set_state(&mut self, new_state: AppState) {
    self.state = Some(new_state);
    self.queue_draw();
  }

  fn on_device_discovered(&mut self, device: DiscoveredDevice) {
    let Some(server) = self.server.as_ref() else {
      log::warn!("Received discovered device before server was initialized");
      return;
    };

    let app_device = AppDevice::from_discovered(device, server);
    // Replace existing entry if address matches first; otherwise fall back to name.
    if let Some(existing) =
      self
        .devices
        .iter_mut()
        .find(|d| match (d.address(), app_device.address()) {
          (Some(existing), Some(new)) => existing == new,
          _ => false,
        })
    {
      *existing = app_device;
    } else if let Some(existing) = self
      .devices
      .iter_mut()
      .find(|d| d.name() == app_device.name())
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
        let connected_index = if let Some(index) = self
          .devices
          .iter()
          .position(|d| d.buttplug_index() == Some(device_index))
        {
          self.devices[index].set_connected_device(device);
          index
        } else if let Some(index) = self.pending_connect {
          self.devices[index].set_connected_device(device);
          index
        } else {
          log::error!("Received DeviceAdded for unknown device index {}, and no pending connect", device_index);
          return;
        };

        if self.pending_connect.is_some() {
          self.pending_connect = None;
          self.current_device_index = Some(connected_index);
          self.goto_device_control();
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

              if let Some(AppState::DeviceControl(_)) = self.state {
                log::info!("Current device removed, returning to device list");
                self.goto_device_list();
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
        self.ensure_device_list_scanning().await;
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
        self.ensure_device_list_scanning().await;
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
      Some(AppState::DeviceList(state)) => {
        if state.last_activity.elapsed() >= std::time::Duration::from_secs(60) {
          log::info!("Device list idle timeout, returning to idle");
          self.goto_idle();
        } else {
          self.state = Some(AppState::DeviceList(state));
          self.ensure_device_list_scanning().await;
        }
      }
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

    self.display.flush()?;

    Ok(())
  }
}
