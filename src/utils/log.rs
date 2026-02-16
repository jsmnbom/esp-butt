pub struct TimeOnlyTimer {}

impl tracing_subscriber::fmt::time::FormatTime for TimeOnlyTimer {
  fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> core::fmt::Result {
    let now = unsafe { esp_idf_svc::sys::esp_timer_get_time() };
    let seconds = now / 1_000_000;
    let micros = now % 1_000_000;
    let hours = (seconds / 3600) % 24;
    let minutes = (seconds / 60) % 60;
    let seconds = seconds % 60;

    write!(w, "{hours:02}:{minutes:02}:{seconds:02}.{micros:06}")
  }
}
