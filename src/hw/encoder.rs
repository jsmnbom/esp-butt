use esp_idf_svc::hal::gpio::{InputPin, InterruptType};
use esp_idf_svc::sys;
use esp_idf_svc::sys::{EspError, esp};
use futures::Stream;
use tokio::sync::broadcast;

use crate::esp_timer_create;
use crate::hw::gpio_input_config;
use crate::{
  app::{AppEvent, NavigationEvent},
  gpio_isr_handler_add,
  utils,
};

pub struct Encoder {
  tx: broadcast::Sender<AppEvent>,
}

const BUTTON_TICK_MS: u64 = 10;
const BUTTON_DEBOUNCE_TICKS: u8 = 3;

enum ButtonState {
  PressDownCheck,
  PressUpCheck,
}

struct Button {
  pin: i32,
  tx: broadcast::Sender<AppEvent>,
  level: bool,
  debounce: u8,
  state: ButtonState,
  timer_handle: sys::esp_timer_handle_t,
  timer_active: bool,
}

impl Encoder {
  pub fn new(
    pin_a: impl InputPin + 'static,
    pin_b: impl InputPin + 'static,
    btn: impl InputPin + 'static,
  ) -> Result<Self, EspError> {
    let (tx, _) = broadcast::channel(16);

    gpio_input_config(&pin_a, Some(InterruptType::AnyEdge))?;
    gpio_input_config(&pin_b, None)?;
    gpio_input_config(&btn, Some(InterruptType::NegEdge))?;

    let pin_a_arg = Box::into_raw(Box::new((pin_a.pin() as u8, pin_b.pin() as u8, tx.clone())));
    gpio_isr_handler_add!(pin_a, Encoder::pin_a_isr_handler, pin_a_arg)?;

    let btn_arg = Box::into_raw(Box::new(Button {
      pin: btn.pin() as i32,
      tx: tx.clone(),
      level: true,
      debounce: 0,
      state: ButtonState::PressDownCheck,
      timer_handle: core::ptr::null_mut(),
      timer_active: false,
    }));

    let btn_timer_handle = esp_timer_create!(Encoder::btn_handler, encoder_button, btn_arg)?;

    {
      let btn = unsafe { &mut *(btn_arg as *mut Button) };
      btn.timer_handle = btn_timer_handle;
    }

    gpio_isr_handler_add!(btn, Encoder::btn_handler, btn_arg)?;

    Ok(Encoder { tx })
  }

  pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
    self.tx.subscribe()
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    utils::stream::convert_broadcast_receiver_to_stream(self.subscribe())
  }  

  fn btn_handler(btn: &mut Button) {
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
          let _ = btn.tx.send(AppEvent::Navigation(NavigationEvent::Select));
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

  fn pin_a_isr_handler((pin_a, pin_b, tx): &mut (u8, u8, broadcast::Sender<AppEvent>)) {
    let a = unsafe { sys::gpio_get_level((*pin_a).into()) != 0 };
    let b = unsafe { sys::gpio_get_level((*pin_b).into()) != 0 };

    // Clockwise: { a: false, b: false } followed by { a: true, b: true } -> Down
    // Counterclockwise: { a: false, b: true } followed by { a: true, b: false } - Up

    let _ = tx.send(AppEvent::Navigation(if a && b {
      NavigationEvent::Down
    } else if !a && b {
      NavigationEvent::Up
    } else {
      return;
    }));
  }
}

