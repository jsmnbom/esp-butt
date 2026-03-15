use std::sync::{Arc, atomic::AtomicU16};

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

use crate::{app::{AppEvent, SliderEvent}, utils};

pub const SLIDER_MAX_VALUE: u16 = 4095;

const CHANNELS: usize = 2;
const SAMPLES_PER_CHANNEL: usize = 256;
const TOTAL_SAMPLES: usize = CHANNELS * SAMPLES_PER_CHANNEL;
const HYSTERESIS_THRESHOLD: u16 = 16;

pub struct SliderChannels<C1, C2>(pub (C1, C2));

impl<ADC: AdcUnit, C1, C2> AdcChannels for SliderChannels<C1, C2>
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

pub struct Sliders {
  values: Arc<[AtomicU16; CHANNELS]>,
  notifier: Arc<Notify>,
}

fn to_range(value: u16, max: u16) -> u16 {
  (value as u32 * max as u32 / SLIDER_MAX_VALUE as u32) as u16
}

impl Sliders {
  pub fn new<ADC: AdcUnit>(
    adc_unit: impl Adc<AdcUnit = ADC> + 'static,
    pins: (
      impl ADCPin<AdcChannel = impl AdcChannel<AdcUnit = ADC>> + 'static,
      impl ADCPin<AdcChannel = impl AdcChannel<AdcUnit = ADC>> + 'static,
    ),
  ) -> Result<Self, EspError> {
    let config = AdcContConfig {
      // Don't buffer samples, as we want to read them as fast as possible and don't care about losing some if we can't keep up
      frames_count: 1,
      // Sample at 20 kHz
      sample_freq: esp_idf_svc::hal::units::Hertz(20000),
      frame_measurements: TOTAL_SAMPLES,
    };

    let channels = SliderChannels((Attenuated::db12(pins.0), Attenuated::db12(pins.1)));
    let mut adc = AdcContDriver::new(adc_unit, &config, channels)?;
    adc.start()?;

    let values = Arc::new([const { AtomicU16::new(0) }; CHANNELS]);
    let notifier = Arc::new(Notify::new());

    Self::spawn_task(adc, values.clone(), notifier.clone());

    Ok(Self { values, notifier })
  }

  pub fn stream(&self) -> impl Stream<Item = AppEvent> + use<> {
    let values = self.values.clone();
    let notifier = self.notifier.clone();
    stream! {
      let mut last_values_sent = [0u16; CHANNELS];
      loop {
        notifier.notified().await;

        for i in 0..CHANNELS {
          let value = values[i].load(std::sync::atomic::Ordering::Relaxed);

          if value != last_values_sent[i] {
            log::info!("Slider {} changed to {:<4} -> {:<4} | 0-5: {:<2} | 0-10: {:<2} | 0-20: {:<2}", i, last_values_sent[i], value, to_range(value, 5), to_range(value, 10), to_range(value, 20));
            yield AppEvent::Slider(SliderEvent::Changed(i as u8, value));
            // Update the last sent value for this channel
            last_values_sent[i] = value;
          }
        }
      }
    }
  }

  fn spawn_task(
    mut adc: AdcContDriver<'static>,
    values: Arc<[AtomicU16; CHANNELS]>,
    notifier: Arc<Notify>,
  ) {
    let mut samples = Box::new([AdcMeasurement::default(); TOTAL_SAMPLES]);

    utils::task::spawn(
      async move {
        let mut channel_sum = [0u32; CHANNELS];
        let mut channel_count = [0u32; CHANNELS];
        let mut last_channel_output = [0u16; CHANNELS];
        let mut channel_average = [0u16; CHANNELS];

        loop {
          if let Ok(num_read) = adc.read_async(&mut samples[..]).await {
            channel_sum.fill(0);
            channel_count.fill(0);

            for sample in &samples[..num_read] {
              let channel = sample.channel() as usize;
              if channel < CHANNELS {
                channel_sum[channel] += sample.data() as u32;
                channel_count[channel] += 1;
              }
            }

            for i in 0..CHANNELS {
              if channel_count[i] > 0 {
                channel_average[i] = (channel_sum[i] / channel_count[i]) as u16;
              }

              let deviation = (channel_average[i] as i32 - last_channel_output[i] as i32).abs();

              if deviation > HYSTERESIS_THRESHOLD as i32 {
                values[i].store(channel_average[i], std::sync::atomic::Ordering::Relaxed);
                notifier.notify_waiters();
                last_channel_output[i] = channel_average[i];
              }
            }
          }
          utils::task::sleep(core::time::Duration::from_millis(20)).await;
        }
      },
      c"slider",
      4 * 1024,
      utils::task::Core::App,
      1,
    );
  }
}
