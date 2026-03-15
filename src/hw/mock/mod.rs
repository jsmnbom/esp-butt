mod display;

use std::{
  collections::VecDeque,
  io::{self, Write},
  sync::mpsc,
  thread,
  time::Duration,
};

pub use display::*;

use anyhow::Context;
use futures::Stream;
use image::{DynamicImage, ImageBuffer, Luma};
use ratatui::{
  Terminal,
  backend::CrosstermBackend,
  crossterm::{
    event::{
      self,
      DisableMouseCapture,
      EnableMouseCapture,
      Event,
      KeyCode,
      KeyEventKind,
      MouseButton,
      MouseEventKind,
    },
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
  },
  layout::{Constraint, Direction, Layout, Rect},
  widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use tokio::sync::{broadcast, watch};

use crate::{
  app::{AppEvent, NavigationEvent, SliderEvent},
  utils,
};

pub const SLIDER_MAX_VALUE: u16 = 4095;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
const BUFFER_SIZE: usize = (WIDTH * HEIGHT / 8) as usize;
const LOG_CAPACITY: usize = 500;

pub struct HardwareMock {
  display: Option<Display>,
  input_tx: broadcast::Sender<AppEvent>,
  log_tx: mpsc::Sender<String>,
}

impl HardwareMock {
  pub fn new() -> anyhow::Result<Self> {
    let (input_tx, _) = broadcast::channel(64);
    let (frame_tx, frame_rx) = watch::channel(vec![0; BUFFER_SIZE]);
    let (log_tx, log_rx) = mpsc::channel::<String>();

    let submit_frame_tx = frame_tx.clone();

    Self::spawn_ticker_task(input_tx.clone());
    Self::spawn_ui_task(input_tx.clone(), frame_rx, log_rx)?;

    Ok(Self {
      display: Some(Display::new(move |frame| {
        submit_frame_tx
          .send(frame.to_vec())
          .map_err(|e| anyhow::anyhow!("failed to publish frame: {e}"))
      })?),
      input_tx,
      log_tx,
    })
  }

  fn spawn_ui_task(
    input_tx: broadcast::Sender<AppEvent>,
    frame_rx: watch::Receiver<Vec<u8>>,
    log_rx: mpsc::Receiver<String>,
  ) -> anyhow::Result<()> {
    thread::Builder::new()
      .name("mock-ui".into())
      .spawn(move || {
        if let Err(err) = run_ui(input_tx, frame_rx, log_rx) {
          eprintln!("mock ui exited: {err:?}");
        }
      })
      .context("failed to spawn mock ui thread")?;

    Ok(())
  }

  fn spawn_ticker_task(input_tx: broadcast::Sender<AppEvent>) {
    utils::task::spawn(
      async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        loop {
          let _ = input_tx.send(AppEvent::Tick);
          tokio::time::sleep(Duration::from_secs(60)).await;
        }
      },
      c"mock-ticker",
      2048,
      utils::task::Core::App,
      1,
    );
  }

  pub fn take_display(&mut self) -> Display {
    self.display.take().expect("mock display already taken")
  }

  pub fn input_event_stream(&self) -> std::pin::Pin<Box<dyn Stream<Item = AppEvent> + Send>> {
    Box::pin(utils::stream::convert_broadcast_receiver_to_stream(
      self.input_tx.subscribe(),
    ))
  }

  pub fn log_sender(&self) -> mpsc::Sender<String> {
    self.log_tx.clone()
  }
}

fn run_ui(
  input_tx: broadcast::Sender<AppEvent>,
  mut frame_rx: watch::Receiver<Vec<u8>>,
  log_rx: mpsc::Receiver<String>,
) -> anyhow::Result<()> {
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  ratatui::crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
  let initial_frame = frame_rx.borrow().clone();
  let mut state = UiState {
    logs: VecDeque::with_capacity(LOG_CAPACITY),
    image: picker.new_resize_protocol(frame_to_image(&initial_frame)),
    picker,
    frame: initial_frame,
    slider_values: [0, 0],
    slider_hitboxes: [Rect::ZERO, Rect::ZERO],
  };

  let result = ui_loop(&mut terminal, &input_tx, &mut frame_rx, log_rx, &mut state);

  disable_raw_mode()?;
  ratatui::crossterm::execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen,
    DisableMouseCapture
  )?;
  terminal.show_cursor()?;

  result
}

pub struct LogWriter {
  log_tx: mpsc::Sender<String>,
  pending: Vec<u8>,
}

impl LogWriter {
  pub fn new(log_tx: mpsc::Sender<String>) -> Self {
    Self {
      log_tx,
      pending: Vec::with_capacity(256),
    }
  }

  fn flush_lines(&mut self) {
    while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
      let line = self.pending.drain(..=pos).collect::<Vec<_>>();
      let msg = String::from_utf8_lossy(&line).trim().to_string();
      if !msg.is_empty() {
        let _ = self.log_tx.send(msg);
      }
    }
  }
}

impl Write for LogWriter {
  fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
    self.pending.extend_from_slice(buf);
    self.flush_lines();
    Ok(buf.len())
  }

  fn flush(&mut self) -> io::Result<()> {
    if !self.pending.is_empty() {
      let msg = String::from_utf8_lossy(&self.pending).trim().to_string();
      self.pending.clear();
      if !msg.is_empty() {
        let _ = self.log_tx.send(msg);
      }
    }
    Ok(())
  }
}

struct UiState {
  logs: VecDeque<String>,
  image: StatefulProtocol,
  picker: Picker,
  frame: Vec<u8>,
  slider_values: [u16; 2],
  slider_hitboxes: [Rect; 2],
}

fn ui_loop(
  terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
  input_tx: &broadcast::Sender<AppEvent>,
  frame_rx: &mut watch::Receiver<Vec<u8>>,
  log_rx: mpsc::Receiver<String>,
  state: &mut UiState,
) -> anyhow::Result<()> {
  loop {
    while let Ok(line) = log_rx.try_recv() {
      state.logs.push_back(line);
      while state.logs.len() > LOG_CAPACITY {
        state.logs.pop_front();
      }
    }

    if frame_rx.has_changed().unwrap_or(false) {
      state.frame = frame_rx.borrow_and_update().clone();
      state.image = state
        .picker
        .new_resize_protocol(frame_to_image(&state.frame));
    }

    terminal.draw(|frame| draw_ui(frame, state))?;
    if let Some(result) = state.image.last_encoding_result() {
      let _ = result;
    }

    if event::poll(Duration::from_millis(33))? {
      let event = event::read()?;
      if handle_event(event, state, input_tx) {
        return Ok(());
      }
    }
  }
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, state: &mut UiState) {
  let columns = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Min(48), Constraint::Length(14)])
    .split(frame.area());

  let left = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(14), Constraint::Min(8)])
    .split(columns[0]);

  let display_block = Block::default().title("Display").borders(Borders::ALL);
  let display_area = display_block.inner(left[0]);
  display_block.render(left[0], frame.buffer_mut());

  let image = StatefulImage::default().resize(Resize::Scale(None));
  frame.render_stateful_widget(image, display_area, &mut state.image);

  let sidebar_block = Block::default().title("Controls").borders(Borders::ALL);
  let sidebar_area = sidebar_block.inner(columns[1]);
  sidebar_block.render(columns[1], frame.buffer_mut());

  let sidebar_rows = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(4), Constraint::Min(8)])
    .split(sidebar_area);

  let slider_columns = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(sidebar_rows[1]);

  let help = Paragraph::new("Arrows/Enter\nEsc/q quit\nDrag sliders").wrap(Wrap { trim: true });
  help.render(sidebar_rows[0], frame.buffer_mut());

  state.slider_hitboxes[0] = draw_slider(frame, slider_columns[0], "S0", state.slider_values[0]);
  state.slider_hitboxes[1] = draw_slider(frame, slider_columns[1], "S1", state.slider_values[1]);

  let log_block = Block::default().title("Logs").borders(Borders::ALL);
  let log_area = log_block.inner(left[1]);
  log_block.render(left[1], frame.buffer_mut());

  let lines = state
    .logs
    .iter()
    .rev()
    .take(log_area.height as usize)
    .cloned()
    .collect::<Vec<_>>()
    .into_iter()
    .rev()
    .collect::<Vec<_>>()
    .join("\n");

  let logs = Paragraph::new(lines).wrap(Wrap { trim: false });
  logs.render(log_area, frame.buffer_mut());
}

fn draw_slider(frame: &mut ratatui::Frame<'_>, area: Rect, label: &str, value: u16) -> Rect {
  let ratio = value as f32 / 4095.0;
  let fill_height = (area.height.saturating_sub(2) as f32 * ratio).round() as u16;

  let block = Block::default()
    .title(format!("{label}: {value}"))
    .borders(Borders::ALL);
  let inner = block.inner(area);
  block.render(area, frame.buffer_mut());

  for y in inner.top()..inner.bottom() {
    for x in inner.left()..inner.right() {
      let fill_start = inner.bottom().saturating_sub(fill_height);
      let filled = y >= fill_start;
      if let Some(cell) = frame.buffer_mut().cell_mut((x, y)) {
        if filled {
          cell.set_symbol("█");
        } else {
          cell.set_symbol("░");
        }
      }
    }
  }

  inner
}

fn handle_event(event: Event, state: &mut UiState, input_tx: &broadcast::Sender<AppEvent>) -> bool {
  match event {
    Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
      KeyCode::Esc | KeyCode::Char('q') => {
        let _ = input_tx.send(AppEvent::Quit);
        true
      }
      KeyCode::Up => {
        let _ = input_tx.send(AppEvent::Navigation(NavigationEvent::Up));
        false
      }
      KeyCode::Down => {
        let _ = input_tx.send(AppEvent::Navigation(NavigationEvent::Down));
        false
      }
      KeyCode::Enter => {
        let _ = input_tx.send(AppEvent::Navigation(NavigationEvent::Select));
        false
      }
      KeyCode::Char('1') => {
        send_slider(state, input_tx, 0, state.slider_values[0]);
        false
      }
      KeyCode::Char('2') => {
        send_slider(state, input_tx, 1, state.slider_values[1]);
        false
      }
      KeyCode::Char('+') | KeyCode::Char('=') => {
        state.slider_values[0] = state.slider_values[0].saturating_add(64).min(4095);
        send_slider(state, input_tx, 0, state.slider_values[0]);
        false
      }
      KeyCode::Char('-') => {
        state.slider_values[0] = state.slider_values[0].saturating_sub(64);
        send_slider(state, input_tx, 0, state.slider_values[0]);
        false
      }
      _ => false,
    },
    Event::Mouse(mouse) => {
      if matches!(
        mouse.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
      ) {
        update_slider_from_mouse(state, input_tx, mouse.column, mouse.row, 0);
        update_slider_from_mouse(state, input_tx, mouse.column, mouse.row, 1);
      }
      false
    }
    _ => false,
  }
}

fn update_slider_from_mouse(
  state: &mut UiState,
  input_tx: &broadcast::Sender<AppEvent>,
  column: u16,
  row: u16,
  idx: usize,
) {
  let hitbox = state.slider_hitboxes[idx];
  if hitbox.width == 0 || hitbox.height == 0 {
    return;
  }

  if column >= hitbox.left()
    && column < hitbox.right()
    && row >= hitbox.top()
    && row < hitbox.bottom()
  {
    let rel = 1.0 - (row - hitbox.top()) as f32 / hitbox.height.saturating_sub(1).max(1) as f32;
    let value = (rel * 4095.0).round() as u16;
    state.slider_values[idx] = value;
    send_slider(state, input_tx, idx, value);
  }
}

fn send_slider(
  _state: &mut UiState,
  input_tx: &broadcast::Sender<AppEvent>,
  idx: usize,
  value: u16,
) {
  let _ = input_tx.send(AppEvent::Slider(SliderEvent::Changed(
    idx as u8,
    value.min(4095),
  )));
}

fn frame_to_image(buffer: &[u8]) -> DynamicImage {
  let mut img = ImageBuffer::<Luma<u8>, Vec<u8>>::new(WIDTH, HEIGHT);

  for x in 0..WIDTH {
    for page in 0..(HEIGHT / 8) {
      let byte_index = (page * WIDTH + x) as usize;
      let byte = buffer.get(byte_index).copied().unwrap_or_default();
      for bit in 0..8 {
        let y = page * 8 + bit;
        if y < HEIGHT {
          let on = ((byte >> bit) & 1) == 1;
          let pixel = if on { 255 } else { 0 };
          img.put_pixel(x, y, Luma([pixel]));
        }
      }
    }
  }

  DynamicImage::ImageLuma8(img)
}
