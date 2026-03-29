#[cfg(target_os = "espidf")]
mod task;
#[cfg(target_os = "espidf")]
mod task_info;
#[cfg(not(target_os = "espidf"))]
mod task_mock;

#[cfg(target_os = "espidf")]
pub use task::{sleep, sleep_timer_async, spawn};
#[cfg(target_os = "espidf")]
pub use task_info::TaskInfo;
#[cfg(not(target_os = "espidf"))]
pub use task_mock::{sleep, sleep_timer_async, spawn};

#[derive(Debug, Clone, Copy)]
pub enum Core {
  Pro,
  App,
}
