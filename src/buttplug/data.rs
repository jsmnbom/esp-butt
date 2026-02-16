use std::collections::HashMap;

use buttplug_server_device_config::DeviceConfigurationManager;

use crate::utils::heap::with_spiram;

static BASE_COMMUNICATION_SPECIFIERS: &[u8] =
  include_bytes!("./data/base_communication_specifiers.postcard.bin");
static BASE_DEVICE_DEFINITIONS: &[u8] =
  include_bytes!("./data/base_device_definitions.postcard.bin");

pub fn load_buttplug_data() -> anyhow::Result<DeviceConfigurationManager> {
  // This data is safe to put in SPIRAM since it shouldn't contain any atomics
  let (base_communication_specifiers, base_device_definitions) =
    with_spiram(|| -> anyhow::Result<_> {
      let base_communication_specifiers: Vec<_> =
        postcard::from_bytes(BASE_COMMUNICATION_SPECIFIERS)?;
      let base_device_definitions: HashMap<_, _> = postcard::from_bytes(BASE_DEVICE_DEFINITIONS)?;

      Ok((base_communication_specifiers, base_device_definitions))
    })?;
  // Contains Dashmap, which is not safe to put in SPIRAM due to its atomics, so we need to construct it in normal RAM.
  Ok(DeviceConfigurationManager::new(
    base_communication_specifiers,
    base_device_definitions,
  ))
}
