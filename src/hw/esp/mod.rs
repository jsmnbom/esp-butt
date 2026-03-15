mod display;
mod encoder;
mod slider;
mod ticker;

pub use display::{Display, DisplayCanvas};
pub use encoder::Encoder;
use esp_idf_svc::{
  hal::{
    self,
    gpio::{InputPin, InterruptType},
  },
  sys::{self, EspError, esp},
};
pub use slider::{SLIDER_MAX_VALUE, Sliders};
pub use ticker::Ticker;

pub fn init() -> Result<(), EspError> {
  esp!(unsafe { sys::gpio_install_isr_service(0 as _) })?;
  unsafe { hal::gpio::set_isr_service_flag_unchecked() };

  Ok(())
}

pub fn gpio_input_config(
  pin: &(impl InputPin + 'static),
  intr_type: Option<InterruptType>,
) -> Result<(), EspError> {
  esp!(unsafe {
    sys::gpio_config(&sys::gpio_config_t {
      pin_bit_mask: (1u64 << pin.pin()),
      mode: sys::gpio_mode_t_GPIO_MODE_INPUT as u32,
      pull_up_en: 1,
      pull_down_en: 0,
      intr_type: intr_type.map_or(0, |t| t.into()),
    })
  })
}

#[macro_export]
macro_rules! gpio_isr_handler_add {
  ($pin:expr, $handler:path, $arg_ptr:expr) => {{
    use pastey::paste;

    paste! {
      unsafe extern "C" fn [<$pin _gpio_isr_wrapper>](ctx: *mut core::ffi::c_void) {
        let arg = unsafe { &mut *(ctx as *mut _) };
        $handler(arg)
      }

      unsafe {
        esp!(sys::gpio_isr_handler_add(
          $pin.pin().into(),
          Some([<$pin _gpio_isr_wrapper>]),
          $arg_ptr as *mut core::ffi::c_void
        ))
      }
    }
  }};
}

#[macro_export]
macro_rules! esp_timer_create {
  ($handler:path, $name:ident, $arg_ptr:expr) => {{
    use pastey::paste;

    paste! {
      unsafe extern "C" fn [<$name _timer_wrapper>](ctx: *mut core::ffi::c_void) {
        let arg = unsafe { &mut *(ctx as *mut _) };
        $handler(arg)
      }

      let mut timer_handle: sys::esp_timer_handle_t = core::ptr::null_mut();

      unsafe {
        esp!(sys::esp_timer_create(
          &sys::esp_timer_create_args_t {
            callback: Some([<$name _timer_wrapper>]),
            arg: $arg_ptr as *mut core::ffi::c_void,
            dispatch_method: sys::esp_timer_dispatch_t_ESP_TIMER_TASK,
            name: concat!(stringify!($name), "\0").as_ptr() as *const u8,
            skip_unhandled_events: false as _,
          },
          &mut timer_handle
        )).map(|_| timer_handle)
      }
    }
  }};
}
