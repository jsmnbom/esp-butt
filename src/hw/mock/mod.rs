mod display;

use std::{io, path::PathBuf, thread, time::Duration};

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
  widgets::{Block, Borders, Widget},
};
use ratatui_image::{Resize, StatefulImage, picker::Picker, protocol::StatefulProtocol};
use tokio::sync::{broadcast, watch};

use crate::{
  app::{AppEvent, NavigationEvent, SliderEvent},
  utils,
};

pub use display::*;

pub const SLIDER_MAX_VALUE: u16 = 4095;

const WIDTH: u32 = 128;
const HEIGHT: u32 = 64;
const BUFFER_SIZE: usize = (WIDTH * HEIGHT / 8) as usize;

pub struct HardwareMock {
  pub display: Display,
  pub input_stream: std::pin::Pin<Box<dyn Stream<Item = AppEvent> + Send>>,
}

impl HardwareMock {
  pub fn new() -> anyhow::Result<Self> {
    let (input_tx, _) = broadcast::channel(64);
    let (frame_tx, frame_rx) = watch::channel(vec![0; BUFFER_SIZE]);

    let submit_frame_tx = frame_tx.clone();

    Self::spawn_ticker_task(input_tx.clone());

    if std::env::var_os("ESP_BUTT_RECORD_SESSION").is_some() {
      let gif_path = std::env::temp_dir().join("esp-butt-session.gif");
      let ndjson_path = std::env::temp_dir().join("esp-butt-session.ndjson");
      log::info!("Recording session to {} / {}", gif_path.display(), ndjson_path.display());
      Self::spawn_recorder_task(gif_path, ndjson_path, input_tx.subscribe(), frame_rx.clone());
    }

    Self::spawn_ui_task(input_tx.clone(), frame_rx)?;

    Ok(Self {
      display: Display::new(move |frame| {
        submit_frame_tx
          .send(frame.to_vec())
          .map_err(|e| anyhow::anyhow!("failed to publish frame: {e}"))
      })?,
      input_stream: Box::pin(utils::stream::convert_broadcast_receiver_to_stream(
        input_tx.subscribe(),
      )),
    })
  }

  fn spawn_ui_task(
    input_tx: broadcast::Sender<AppEvent>,
    frame_rx: watch::Receiver<Vec<u8>>,
  ) -> anyhow::Result<()> {
    thread::Builder::new()
      .name("mock-ui".into())
      .spawn(move || {
        if let Err(err) = run_ui(input_tx, frame_rx) {
          eprintln!("mock ui exited: {err:?}");
        }
      })
      .context("failed to spawn mock ui thread")?;

    Ok(())
  }

  fn spawn_ticker_task(input_tx: broadcast::Sender<AppEvent>) {
    utils::task::spawn(
      async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        loop {
          let _ = input_tx.send(AppEvent::Tick);
          tokio::time::sleep(Duration::from_secs(1)).await;
        }
      },
      c"mock-ticker",
      2048,
      utils::task::Core::App,
      1,
    );
  }

  fn spawn_recorder_task(
    gif_path: PathBuf,
    ndjson_path: PathBuf,
    mut event_rx: broadcast::Receiver<AppEvent>,
    mut frame_rx: watch::Receiver<Vec<u8>>,
  ) {
    use image::Frame;
    use image::codecs::gif::{GifEncoder, Repeat};
    use std::io::Write as _;
    use std::time::Instant;

    // Fixed placeholder delay — the GIF is used as a frame container only;
    // actual timing comes from the ndjson event log.
    const FRAME_DELAY_MS: u32 = 100;

    utils::task::spawn(
      async move {
        let gif_file = match std::fs::File::create(&gif_path) {
          Ok(f) => f,
          Err(err) => {
            log::error!("recorder: failed to create gif: {err}");
            return;
          }
        };
        let ndjson_file = match std::fs::File::create(&ndjson_path) {
          Ok(f) => f,
          Err(err) => {
            log::error!("recorder: failed to create ndjson: {err}");
            return;
          }
        };

        let mut encoder = GifEncoder::new_with_speed(std::io::BufWriter::new(gif_file), 10);
        if let Err(err) = encoder.set_repeat(Repeat::Infinite) {
          log::error!("recorder: failed to set repeat: {err}");
          return;
        }
        let mut writer = std::io::BufWriter::new(ndjson_file);

        let start = Instant::now();
        let mut frame_count = 0u32;

        loop {
          tokio::select! {
            result = event_rx.recv() => {
              let t = start.elapsed().as_secs_f64();
              match result {
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Ok(AppEvent::Quit) => {
                  let _ = writer.flush();
                  break;
                }
                Ok(AppEvent::Navigation(nav)) => {
                  let nav_str = match nav {
                    NavigationEvent::Up => "Up",
                    NavigationEvent::Down => "Down",
                    NavigationEvent::Select => "Select",
                  };
                  let _ = writeln!(writer, r#"{{"t":{t:.3},"type":"nav","event":"{nav_str}"}}"#);
                }
                Ok(AppEvent::Slider(SliderEvent::Changed(idx, val))) => {
                  let _ = writeln!(
                    writer,
                    r#"{{"t":{t:.3},"type":"slider","index":{idx},"value":{val}}}"#
                  );
                }
                _ => {}
              }
            }
            Ok(()) = frame_rx.changed() => {
              let t = start.elapsed().as_secs_f64();
              let frame_data = frame_rx.borrow_and_update().clone();
              let rgba = frame_to_image(&frame_data).to_rgba8();
              let gif_frame = Frame::from_parts(
                rgba,
                0,
                0,
                image::Delay::from_numer_denom_ms(FRAME_DELAY_MS, 1),
              );
              if let Err(err) = encoder.encode_frame(gif_frame) {
                log::warn!("recorder: encode error: {err}");
              } else {
                let _ = writeln!(writer, r#"{{"t":{t:.3},"type":"frame","frame":{frame_count}}}"#);
                frame_count += 1;
              }
            }
            else => break,
          }
        }

        log::info!("recorder: finished ({frame_count} frames)");
      },
      c"recorder",
      16384,
      utils::task::Core::App,
      1,
    );
  }
}

fn run_ui(
  input_tx: broadcast::Sender<AppEvent>,
  mut frame_rx: watch::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
  enable_raw_mode()?;
  let mut stdout = io::stdout();
  ratatui::crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

  let backend = CrosstermBackend::new(stdout);
  let mut terminal = Terminal::new(backend)?;

  let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());
  let initial_frame = frame_rx.borrow().clone();

  // Compute the cell dimensions for the 128×64 display image.
  let (font_w, font_h) = picker.font_size();
  let display_cols = (WIDTH as u16).div_ceil(font_w.max(1)) * 2;
  let display_rows = (HEIGHT as u16).div_ceil(font_h.max(1)) * 2;

  let mut state = UiState {
    image: picker.new_resize_protocol(frame_to_image(&initial_frame)),
    picker,
    frame: initial_frame,
    slider_values: [0, 0],
    slider_hitboxes: [Rect::ZERO, Rect::ZERO],
    display_cols,
    display_rows,
  };

  let result = ui_loop(&mut terminal, &input_tx, &mut frame_rx, &mut state);

  disable_raw_mode()?;
  ratatui::crossterm::execute!(
    terminal.backend_mut(),
    LeaveAlternateScreen,
    DisableMouseCapture
  )?;
  terminal.show_cursor()?;

  result
}

struct UiState {
  image: StatefulProtocol,
  picker: Picker,
  frame: Vec<u8>,
  slider_values: [u16; 2],
  slider_hitboxes: [Rect; 2],
  /// Width of the display image in terminal cells.
  display_cols: u16,
  /// Height of the display image in terminal cells.
  display_rows: u16,
}

fn ui_loop(
  terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
  input_tx: &broadcast::Sender<AppEvent>,
  frame_rx: &mut watch::Receiver<Vec<u8>>,
  state: &mut UiState,
) -> anyhow::Result<()> {
  loop {
    if frame_rx.has_changed().unwrap_or(false) {
      state.frame = frame_rx.borrow_and_update().clone();
      state.image = state
        .picker
        .new_resize_protocol(frame_to_image(&state.frame));
    }

    terminal.draw(|frame| draw_ui(frame, state))?;

    if event::poll(Duration::from_millis(33))? {
      let event = event::read()?;
      if handle_event(event, state, input_tx) {
        return Ok(());
      }
    }
  }
}

fn draw_ui(frame: &mut ratatui::Frame<'_>, state: &mut UiState) {
  let left_col_width = state.display_cols + 2;
  let display_block_height = state.display_rows + 2;

  let left = Layout::default()
    .direction(Direction::Vertical)
    .constraints([Constraint::Length(display_block_height), Constraint::Min(6)])
    .split(Rect::new(0, 0, left_col_width, frame.area().height));

  // Display
  let display_block = Block::default().title("Display").borders(Borders::ALL);
  let display_area = display_block.inner(left[0]);
  display_block.render(left[0], frame.buffer_mut());

  let image = StatefulImage::default().resize(Resize::Scale(None));
  frame.render_stateful_widget(image, display_area, &mut state.image);

  // Sliders — side by side.
  let slider_area = left[1];
  let slider_cols = Layout::default()
    .direction(Direction::Horizontal)
    .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
    .split(slider_area);

  state.slider_hitboxes[0] = draw_slider(
    frame,
    slider_cols[0],
    "S0",
    state.slider_values[0],
    state.display_rows,
  );
  state.slider_hitboxes[1] = draw_slider(
    frame,
    slider_cols[1],
    "S1",
    state.slider_values[1],
    state.display_rows,
  );
}

fn draw_slider(
  frame: &mut ratatui::Frame<'_>,
  area: Rect,
  label: &str,
  value: u16,
  _display_rows: u16,
) -> Rect {
  let ratio = value as f32 / 4095.0;

  let block = Block::default()
    .title(format!("{label}: {value}"))
    .borders(Borders::ALL);
  let inner = block.inner(area);
  block.render(area, frame.buffer_mut());

  // Pixel-accurate fill: the inner area height in cells maps to SLIDER_MAX_VALUE.
  // We use the full inner cell height as the "pixel" resolution.
  let total_cells = inner.height;
  let fill_cells = (total_cells as f32 * ratio).round() as u16;

  for y in inner.top()..inner.bottom() {
    for x in inner.left()..inner.right() {
      let fill_start = inner.bottom().saturating_sub(fill_cells);
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
      match mouse.kind {
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
          update_slider_from_mouse(state, input_tx, mouse.column, mouse.row, 0);
          update_slider_from_mouse(state, input_tx, mouse.column, mouse.row, 1);
        }
        _ => {}
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
