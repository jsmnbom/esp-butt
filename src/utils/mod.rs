use esp_idf_svc::timer::EspTaskTimerService;

pub mod heap;
pub mod log;
pub mod os_mbuf;
pub mod ptr;
pub mod spawn;

pub async fn sleep(duration: core::time::Duration) {
  EspTaskTimerService::new()
    .unwrap()
    .timer_async()
    .unwrap()
    .after(duration)
    .await
    .unwrap();
}
