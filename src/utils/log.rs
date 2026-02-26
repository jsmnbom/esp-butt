use core::sync::atomic::{AtomicU32, Ordering};
use std::fmt::Write as FmtWrite;
use std::io::{self, Write};

use tracing::Level;
use tracing::span;
use tracing_log::NormalizeEvent;

pub const RESET: &str = "\x1b[0m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const YELLOW: &str = "\x1b[33m";
pub const GREEN: &str = "\x1b[32m";
pub const CYAN: &str = "\x1b[36m";
pub const MAGENTA: &str = "\x1b[35m";

fn level_color(level: &Level) -> &'static str {
  match *level {
    Level::ERROR => RED,
    Level::WARN => YELLOW,
    Level::INFO => GREEN,
    Level::DEBUG => CYAN,
    Level::TRACE => MAGENTA,
  }
}

/// A target-prefix → minimum level pair used for per-crate filtering.
pub struct Filter {
  pub prefix: &'static str,
  pub level: Level,
}

/// A compact, ANSI-coloured [`tracing::Subscriber`] tailored for ESP-IDF.
pub struct Subscriber {
  /// Global minimum level (events below this are always dropped).
  max_level: Level,
  /// Per-target-prefix overrides.  First matching prefix wins.
  filters: Vec<Filter>,
  /// Monotonically increasing span ID source.
  next_id: AtomicU32,
}

impl Subscriber {
  pub fn new(max_level: Level) -> Self {
    Self {
      max_level,
      filters: Vec::new(),
      next_id: AtomicU32::new(1),
    }
  }

  /// Add a per-crate level filter.  The first filter whose prefix matches
  /// the event/span target (via [`str::starts_with`]) wins.
  pub fn with_filter(mut self, prefix: &'static str, level: Level) -> Self {
    self.filters.push(Filter { prefix, level });
    self
  }

  /// Install this subscriber as the global default.
  pub fn install(self) {
    tracing::subscriber::set_global_default(self).expect("global tracing subscriber already set");
  }

  fn effective_level(&self, target: &str) -> Level {
    for f in &self.filters {
      if target.starts_with(f.prefix) {
        return f.level;
      }
    }
    self.max_level
  }

  fn format_time(&self, buf: &mut impl FmtWrite) -> core::fmt::Result {
    let now = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    let seconds = now / 1_000_000;
    let micros = now % 1_000_000;
    let hours = (seconds / 3600) % 24;
    let minutes = (seconds / 60) % 60;
    let secs = seconds % 60;
    write!(buf, "{hours:02}:{minutes:02}:{secs:02}.{micros:06}")
  }
}

impl tracing::Subscriber for Subscriber {
  fn enabled(&self, metadata: &tracing::Metadata<'_>) -> bool {
    *metadata.level() <= self.effective_level(metadata.target())
  }

  fn new_span(&self, _attrs: &span::Attributes<'_>) -> span::Id {
    let id = self.next_id.fetch_add(1, Ordering::Relaxed);
    span::Id::from_u64(id as u64)
  }

  fn record(&self, _span: &span::Id, _values: &span::Record<'_>) {}
  fn record_follows_from(&self, _span: &span::Id, _follows: &span::Id) {}
  fn enter(&self, _span: &span::Id) {}
  fn exit(&self, _span: &span::Id) {}

  fn event(&self, event: &tracing::Event<'_>) {
    // normalized_metadata() returns the original log metadata for events
    // bridged from the `log` crate (tracing-log), falling back to the
    // event's own metadata for native tracing events.
    let norm = event.normalized_metadata();
    let meta = norm.as_ref().unwrap_or_else(|| event.metadata());

    // Fast-path: drop if filtered out.
    if !self.enabled(meta) {
      return;
    }

    let level = meta.level();
    let target = meta.target();
    let color = level_color(level);

    // Build the message part by visiting fields.
    let mut msg = String::new();
    let mut visitor = MessageVisitor(&mut msg);
    event.record(&mut visitor);

    // Build the full log line into a String, then write atomically.
    let mut line = String::with_capacity(128);

    // Dimmed timestamp
    let _ = write!(line, "{DIM}");
    let _ = self.format_time(&mut line);
    let _ = write!(line, "{RESET} ");

    // Colored + padded level
    let _ = write!(line, "{color}{:<5}{RESET} ", level.as_str());

    // Target (dimmed)
    let _ = write!(line, "{DIM}{target}{RESET}: ");

    // Message
    let _ = write!(line, "{msg}");

    // Trailing newline + flush via a single write to stderr
    let _ = writeln!(io::stderr().lock(), "{line}");
  }
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
  fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn core::fmt::Debug) {
    let name = field.name();
    if name == "message" {
      let _ = write!(self.0, "{value:?}");
    } else if name.starts_with("log.") {
      return;
    } else {
      let _ = write!(self.0, " {}={value:?}", name);
    }
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    let name = field.name();
    if name == "message" {
      self.0.push_str(value);
    } else if name.starts_with("log.") {
      return;
    } else {
      let _ = write!(self.0, " {}={value}", name);
    }
  }
}
