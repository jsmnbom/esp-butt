use std::ffi::CStr;

use esp_idf_svc::sys;
use strum::FromRepr;

const TSK_NO_AFFINITY: i32 = 0x7FFFFFFF;
const SYSTEM_TASKS: [&str; 9] = [
  "ipc0",
  "ipc1",
  "esp_timer",
  "IDLE0",
  "IDLE1",
  "timersvc",
  "btController",
  "nimble_host",
  "IsrReactor",
];

use crate::utils::log::{DIM, RESET};

#[repr(u8)]
#[derive(Clone, Copy, Debug, FromRepr)]
enum TaskState {
  Running,
  Ready,
  Blocked,
  Suspended,
  Deleted,
  Unknown,
}

impl std::fmt::Display for TaskState {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let s = match self {
      TaskState::Running => "X",
      TaskState::Ready => "R",
      TaskState::Blocked => "B",
      TaskState::Suspended => "S",
      TaskState::Deleted => "D",
      TaskState::Unknown => "?",
    };
    write!(f, "{}", s)
  }
}

pub struct TaskInfo<'a> {
  id: u32,
  name: &'a str,
  core: i32,
  state: TaskState,
  priority: u32,
  cpu: u64,
  stack_hwm: u32,
}

impl<'a> TaskInfo<'a> {
  fn new(value: sys::TaskStatus_t, total_runtime: u32) -> Self {
    Self {
      id: value.xTaskNumber,
      name: unsafe { CStr::from_ptr(value.pcTaskName) }
        .to_str()
        .unwrap_or("?")
        .as_ref(),
      core: unsafe { sys::xTaskGetCoreID(value.xHandle) },
      state: TaskState::from_repr(value.eCurrentState as u8).unwrap(),
      priority: value.uxCurrentPriority,
      cpu: ((value.ulRunTimeCounter as u64 * 100) / total_runtime as u64),
      stack_hwm: value.usStackHighWaterMark,
    }
  }

  fn is_sys(&self) -> bool {
    SYSTEM_TASKS.contains(&self.name)
  }

  pub fn gather() -> Vec<Self> {
    let num_tasks = unsafe { sys::uxTaskGetNumberOfTasks() } as usize;
    let capacity = num_tasks + 4;
    let mut status_array: Vec<sys::TaskStatus_t> = Vec::with_capacity(capacity);
    let mut total_run_time: u32 = 0;

    let filled = unsafe {
      sys::uxTaskGetSystemState(
        status_array.as_mut_ptr(),
        capacity as u32,
        &mut total_run_time,
      )
    } as usize;

    unsafe { status_array.set_len(filled) };

    let mut tasks: Vec<Self> = status_array
      .into_iter()
      .map(|s| Self::new(s, total_run_time))
      .collect();
    tasks.sort_by_key(|t| (t.is_sys(), t.id));

    tasks
  }
}

impl std::fmt::Display for TaskInfo<'_> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let pre = if self.is_sys() { DIM } else { "" };
    let post = if self.is_sys() { RESET } else { "" };
    let core = match self.core {
      0 => "P",
      1 => "A",
      TSK_NO_AFFINITY => " ",
      _ => "?",
    };
    write!(
      f,
      "{}{:>2} │ {:<16} │ {} │ {:>2} │ {} │ {:>2}% │ {:>5}{}",
      pre, self.id, self.name, self.state, self.priority, core, self.cpu, self.stack_hwm, post
    )
  }
}
