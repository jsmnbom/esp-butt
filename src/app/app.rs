use std::pin::Pin;

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
  buttplug,
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
  pub(super) client: buttplug_client::ButtplugClient,

  pub(super) scanning: bool,
  pub(super) devices: Vec<buttplug_client::ButtplugClientDevice>,
  pub(super) state: Option<AppState>,

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
      client: buttplug_client::ButtplugClient::new("esp"),

      scanning: false,
      devices: Vec::new(),
      state: Some(AppState::Idle),
      
      tx,
      input_event_stream: Some(input_event_stream),
    }
  }

  pub async fn main(mut self) -> anyhow::Result<()> {
    let connector = buttplug::create_buttplug()?;

    let client_stream = self
      .client
      .event_stream()
      .map(|event| AppEvent::ButtplugEvent(event));

    let self_stream = utils::stream::convert_broadcast_receiver_to_stream(self.tx.subscribe());

    let mut event_stream = Box::pin(
      (
        client_stream,
        self_stream,
        self.input_event_stream.take().unwrap(),
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

  pub(super) fn goto_device_control(&mut self, device_index: usize) {
    match self.create_device_control(device_index) {
      Ok(state) => self.set_state(AppState::DeviceControl(state)),
      Err(e) => log::error!("Error creating device control state: {:?}", e),
    }
  }

  fn set_state(&mut self, new_state: AppState) {
    self.state = Some(new_state);
    self.queue_draw();
  }

  async fn on_buttplug_event(&mut self, event: &ButtplugClientEvent) {
    match event {
      ButtplugClientEvent::DeviceAdded(device) => {
        log::info!("Device added: {}", device.name());
        self.devices.push(device.clone());
        self.queue_draw();
      }
      ButtplugClientEvent::DeviceRemoved(device) => {
        log::info!("Device removed: {}", device.name());
        self.devices.retain(|d| d.index() != device.index());
        self.queue_draw();

        if let Some(AppState::DeviceControl(ref state)) = self.state {
          if device.index() as usize == state.device_index {
            log::info!("Current device removed, returning to device list");
            self.goto_device_list();
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
