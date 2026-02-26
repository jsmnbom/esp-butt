use buttplug_server_device_config::DeviceConfigurationManager;

static BASE_COMMUNICATION_SPECIFIERS: &[u8] =
  include_bytes!("./data/base_communication_specifiers.postcard.bin.gz");
static BASE_DEVICE_DEFINITIONS: &[u8] =
  include_bytes!("./data/base_device_definitions.postcard.bin.gz");

pub fn load_buttplug_data() -> anyhow::Result<DeviceConfigurationManager> {
  let base_communication_specifiers: Vec<_> = decompress_and_load(BASE_COMMUNICATION_SPECIFIERS)?;
  let base_device_definitions: std::collections::HashMap<_, _> =
    decompress_and_load(BASE_DEVICE_DEFINITIONS)?;
  Ok(DeviceConfigurationManager::new(
    base_communication_specifiers,
    base_device_definitions,
  ))
}

fn decompress_and_load<T>(compressed: &[u8]) -> anyhow::Result<T>
where
  T: serde::de::DeserializeOwned,
{
  let decompressed = miniz_oxide::inflate::decompress_to_vec(compressed)
    .map_err(|_| anyhow::anyhow!("Decompression error!"))?;
  Ok(postcard::from_bytes(&decompressed)?)
}
