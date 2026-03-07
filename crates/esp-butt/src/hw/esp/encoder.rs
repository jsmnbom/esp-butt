use esp_idf_svc::hal::delay::BLOCK;
use esp_idf_svc::hal::gpio::{InputPin, InterruptType};
use esp_idf_svc::hal::task::queue::Queue;
use esp_idf_svc::sys;
use esp_idf_svc::sys::{EspError, esp};
use futures::Stream;
use tokio::sync::broadcast;

use crate::{
  app::{AppEvent, NavigationEvent},
  esp_timer_create,
  gpio_isr_handler_add,
  hw::gpio_input_config,
  utils::{self, task::spawn, task::Core},
};

pub struct Encoder {
  tx: broadcast::Sender<AppEvent>,
}

const BUTTON_TICK_MS: u64 = 15;
const BUTTON_DEBOUNCE_TICKS: u8 = 3;

enum ButtonState {
  PressDownCheck,
  PressUpCheck,
}

/// ISR argument for the rotary encoder pins.
/// Uses raw pointers to the ISR-safe queue and notification instead of the
/// broadcast::Sender, which internally uses a pthread_mutex and must never be
/// called from an ISR context.
struct EncoderArg {
  pin_a: u8,
  pin_b: u8,
  queue: *const Queue<NavigationEvent>,
}

// Safety: PinIsrArg is only ever accessed from a single ISR handler.
unsafe impl Send for EncoderArg {}

struct BtnArg {
  pin: i32,
  queue: *const Queue<NavigationEvent>,
  level: bool,
  debounce: u8,
  state: ButtonState,
  timer_handle: sys::esp_timer_handle_t,
  timer_active: bool,
}

// Safety: Button is only ever accessed from the button ISR / timer callback.
unsafe impl Send for BtnArg {}

impl Encoder {
  pub fn new(
    pin_a: impl InputPin + 'static,
    pin_b: impl InputPin + 'static,
    pin_btn: impl InputPin + 'static,
  ) -> Result<Self, EspError> {
    let (tx, _) = broadcast::channel(16);

    gpio_input_config(&pin_a, Some(InterruptType::AnyEdge))?;
    gpio_input_config(&pin_b, None)?;
    gpio_input_config(&pin_btn, Some(InterruptType::NegEdge))?;

    // Leak the queue so it gets a 'static lifetime that is safe to share
    // with ISR handlers via raw pointer.
    let queue: &'static Queue<NavigationEvent> = Box::leak(Box::new(Queue::new(16)));

    let encoder_arg = Box::into_raw(Box::new(EncoderArg {
      pin_a: pin_a.pin() as u8,
      pin_b: pin_b.pin() as u8,
      queue,
    }));
    gpio_isr_handler_add!(pin_a, Encoder::encoder_isr_handler, encoder_arg)?;

    let btn_arg = Box::into_raw(Box::new(BtnArg {
      pin: pin_btn.pin() as i32,
      queue,
      level: true,
      debounce: 0,
      state: ButtonState::PressDownCheck,
      timer_handle: core::ptr::null_mut(),
      timer_active: false,
    }));

    let btn_timer_handle = esp_timer_create!(Encoder::btn_isr_handler, encoder_button, btn_arg)?;
    let btn = unsafe { &mut *(btn_arg as *mut BtnArg) };
    btn.timer_handle = btn_timer_handle;

    gpio_isr_handler_add!(pin_btn, Encoder::btn_isr_handler, btn_arg)?;

    Self::spawn_task(tx.clone(), queue);

    Ok(Encoder { tx })
  }

  fn spawn_task(tx: broadcast::Sender<AppEvent>, queue: &'static Queue<NavigationEvent>) {
    // Bridge task: blocks on the ISR-safe queue and forwards events to the
    // broadcast channel.  The broadcast::Sender uses a mutex, which is only
    // safe to call from a normal task — never from an ISR.
    // send_back() from the ISR uses xQueueGenericSendFromISR, which wakes the
    // FreeRTOS task blocked here on xQueueReceive(BLOCK) directly.
    spawn(
      async move {
        loop {
          if let Some((event, _)) = queue.recv_front(BLOCK) {
            let _ = tx.send(AppEvent::Navigation(event));
          }
        }
      },
      c"encoder",
      2 * 1024,
      Core::App,
      5,
    );
  }

  pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
    self.tx.subscribe()
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    utils::stream::convert_broadcast_receiver_to_stream(self.subscribe())
  }

  fn btn_isr_handler(btn: &mut BtnArg) {
    let read_level = unsafe { sys::gpio_get_level(btn.pin) != 0 };
    if read_level != btn.level {
      btn.debounce += 1;
      if btn.debounce >= BUTTON_DEBOUNCE_TICKS {
        btn.level = read_level;
        btn.debounce = 0;
      }
    } else {
      btn.debounce = 0;
    }

    match btn.state {
      ButtonState::PressDownCheck => {
        if !btn.level {
          // ISR-safe: send_back uses xQueueGenericSendFromISR when
          // interrupt::active() is true, which wakes the bridge task.
          let queue = unsafe { &*btn.queue };
          let _ = queue.send_back(NavigationEvent::Select, 0);
          btn.state = ButtonState::PressUpCheck;
        }
        if !btn.timer_active {
          unsafe { sys::esp_timer_start_periodic(btn.timer_handle, BUTTON_TICK_MS * 1_000) };
          btn.timer_active = true;
        }
      }
      ButtonState::PressUpCheck => {
        if btn.level {
          btn.state = ButtonState::PressDownCheck;
          if btn.timer_active {
            unsafe { sys::esp_timer_stop(btn.timer_handle) };
            btn.timer_active = false;
          }
        }
      }
    }
  }

  fn encoder_isr_handler(arg: &mut EncoderArg) {
    let a = unsafe { sys::gpio_get_level(arg.pin_a.into()) != 0 };
    let b = unsafe { sys::gpio_get_level(arg.pin_b.into()) != 0 };

    // Clockwise: { a: false, b: false } followed by { a: true, b: true } -> Down
    // Counterclockwise: { a: false, b: true } followed by { a: true, b: false } -> Up

    let event = if a && b {
      NavigationEvent::Down
    } else if !a && b {
      NavigationEvent::Up
    } else {
      return;
    };

    // ISR-safe: send_back uses xQueueGenericSendFromISR when
    // interrupt::active() is true, which wakes the bridge task.
    let queue = unsafe { &*arg.queue };
    let _ = queue.send_back(event, 0);
  }
}
