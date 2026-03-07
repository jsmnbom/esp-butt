
#[cfg(target_os = "espidf")]
mod esp;
#[cfg(target_os = "espidf")]
pub use esp::*;

#[cfg(not(target_os = "espidf"))]
mod mock;
#[cfg(not(target_os = "espidf"))]
pub use mock::*;
