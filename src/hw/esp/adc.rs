use std::sync::{
  Arc,
  atomic::{AtomicU16, Ordering},
};

use async_stream::stream;
use esp_idf_svc::{
  hal::{
    adc::{
      Adc,
      AdcChannel,
      AdcChannels,
      AdcContConfig,
      AdcContDriver,
      AdcMeasurement,
      AdcUnit,
      Attenuated,
    },
    gpio::ADCPin,
  },
  sys::EspError,
};
use futures::Stream;
use tokio::sync::Notify;

use crate::{
  app::{AppEvent, SliderEvent},
  utils,
};

pub const SLIDER_MAX_VALUE: u16 = 4095;

const SLIDER_CHANNELS: usize = 2;
const TOTAL_CHANNELS: usize = 3;
const SAMPLES_PER_CHANNEL: usize = 256;
const TOTAL_SAMPLES: usize = TOTAL_CHANNELS * SAMPLES_PER_CHANNEL;
const HYSTERESIS_THRESHOLD: u16 = 16;

/// Channel index of the battery ADC input (gpio3).
const BATTERY_CHANNEL: usize = 2;

struct ContAdcChannels<C1, C2>(pub (C1, C2));

impl<ADC: AdcUnit, C1, C2> AdcChannels for ContAdcChannels<C1, C2>
where
  C1: AdcChannels<AdcUnit = ADC>,
  C2: AdcChannels<AdcUnit = ADC>,
{
  type AdcUnit = ADC;

  type Iterator<'a>
    = core::iter::Chain<C1::Iterator<'a>, C2::Iterator<'a>>
  where
    Self: 'a;

  fn iter(&self) -> Self::Iterator<'_> {
    self.0.0.iter().chain(self.0.1.iter())
  }
}

pub struct AdcInputs {
  slider_values: Arc<[AtomicU16; SLIDER_CHANNELS]>,
  notifier: Arc<Notify>,
  battery_raw: Arc<AtomicU16>,
}

impl AdcInputs {
  pub fn new<ADC: AdcUnit>(
    adc_unit: impl Adc<AdcUnit = ADC> + 'static,
    slider_pins: (
      impl ADCPin<AdcChannel = impl AdcChannel<AdcUnit = ADC>> + 'static,
      impl ADCPin<AdcChannel = impl AdcChannel<AdcUnit = ADC>> + 'static,
    ),
    battery_pin: impl ADCPin<AdcChannel = impl AdcChannel<AdcUnit = ADC>> + 'static,
  ) -> Result<Self, EspError> {
    let config = AdcContConfig {
      // Don't buffer samples, as we want to read them as fast as possible and don't care about losing some if we can't keep up
      frames_count: 1,
      // Sample at 20 kHz
      sample_freq: esp_idf_svc::hal::units::Hertz(20000),
      frame_measurements: TOTAL_SAMPLES,
    };

    // Channels: slider0 (gpio1), slider1 (gpio2), battery (gpio3)
    let channels = ContAdcChannels((
      ContAdcChannels((
        Attenuated::db12(slider_pins.0),
        Attenuated::db12(slider_pins.1),
      )),
      Attenuated::db12(battery_pin),
    ));
    let mut adc = AdcContDriver::new(adc_unit, &config, channels)?;
    adc.start()?;

    let slider_values = Arc::new([const { AtomicU16::new(0) }; SLIDER_CHANNELS]);
    let notifier = Arc::new(Notify::new());
    let battery_raw = Arc::new(AtomicU16::new(0));

    Self::spawn_task(adc, slider_values.clone(), notifier.clone(), battery_raw.clone());

    Ok(Self {
      slider_values,
      notifier,
      battery_raw,
    })
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    let values = self.slider_values.clone();
    let notifier = self.notifier.clone();
    stream! {
      let mut last_values_sent = [0u16; SLIDER_CHANNELS];
      loop {
        notifier.notified().await;

        for i in 0..SLIDER_CHANNELS {
          let value = values[i].load(Ordering::Acquire);

          if value != last_values_sent[i] {
            yield AppEvent::Slider(SliderEvent::Changed(i as u8, value));
            last_values_sent[i] = value;
          }
        }
      }
    }
  }

  /// Returns the latest raw 12-bit ADC reading from the battery voltage divider.
  /// V_BAT ≈ raw * 20 / 11 - 220 mV  (two-point calibrated: raw 1815 → 3080 mV, raw 2420 → 4180 mV).
  pub fn battery_raw(&self) -> u16 {
    self.battery_raw.load(Ordering::Acquire)
  }

  fn spawn_task(
    mut adc: AdcContDriver<'static>,
    slider_values: Arc<[AtomicU16; SLIDER_CHANNELS]>,
    notifier: Arc<Notify>,
    battery_raw: Arc<AtomicU16>,
  ) {
    let mut samples = Box::new([AdcMeasurement::default(); TOTAL_SAMPLES]);

    utils::task::spawn(
      async move {
        let mut channel_sum = [0u32; TOTAL_CHANNELS];
        let mut channel_count = [0u32; TOTAL_CHANNELS];
        let mut last_slider_output = [0u16; SLIDER_CHANNELS];
        let mut channel_average = [0u16; TOTAL_CHANNELS];

        loop {
          if let Ok(num_read) = adc.read_async(&mut samples[..]).await {
            channel_sum.fill(0);
            channel_count.fill(0);

            for sample in &samples[..num_read] {
              let channel = sample.channel() as usize;
              if channel < TOTAL_CHANNELS {
                channel_sum[channel] += sample.data() as u32;
                channel_count[channel] += 1;
              }
            }

            for i in 0..TOTAL_CHANNELS {
              if channel_count[i] > 0 {
                channel_average[i] = (channel_sum[i] / channel_count[i]) as u16;
              }
            }

            // Slider channels: hysteresis + notify
            for i in 0..SLIDER_CHANNELS {
              let deviation =
                (channel_average[i] as i32 - last_slider_output[i] as i32).unsigned_abs() as u16;

              if deviation > HYSTERESIS_THRESHOLD {
                slider_values[i].store(channel_average[i], Ordering::Release);
                notifier.notify_one();
                last_slider_output[i] = channel_average[i];
              }
            }

            // Battery channel: just store the running average
            if channel_count[BATTERY_CHANNEL] > 0 {
              battery_raw.store(channel_average[BATTERY_CHANNEL], Ordering::Release);
            }
          }
          utils::task::sleep(core::time::Duration::from_millis(25)).await;
        }
      },
      c"adc",
      4 * 1024,
      utils::task::Core::App,
      8,
    );
  }
}
