use crate::utils::{self, heap, task};

#[allow(dead_code)]
pub fn start_reporting(interval: core::time::Duration) {
  task::spawn(
    async move {
      loop {
        utils::task::sleep_timer_async(interval).await;
        log_heap();
        log_task_list();
      }
    },
    c"report",
    6 * 1024,
    task::Core::App,
    1,
  );
}

pub fn log_heap() {
  let internal = heap::HeapRegionStats::internal();
  let external = heap::HeapRegionStats::external();

  log::debug!("Internal RAM: {}", internal);
  log::debug!("External RAM: {}", external);
}

pub fn log_task_list() {
  let tasks = task::TaskInfo::gather();

  log::debug!(
    "{:>2} │ {:<16} │ {} │ {:<2} │ {} │ {:<3} │ {:>5}",
    "ID",
    "Task name",
    "S",
    "P",
    "C",
    "CPU",
    "HWM"
  );
  log::debug!("───┼──────────────────┼───┼────┼───┼─────┼──────");

  for task in tasks.into_iter() {
    log::debug!("{}", task);
  }
}
