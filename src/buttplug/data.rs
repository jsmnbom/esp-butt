use std::{collections::HashMap, sync::Arc};

use buttplug_server_device_config::{
  BaseDeviceIdentifier,
  DeviceConfigurationManager,
  ProtocolCommunicationSpecifier,
  ServerDeviceDefinition,
};
use serde::Deserialize;
use tracing::instrument;

#[cfg(target_os = "espidf")]
use crate::utils::heap::{HEAP, MALLOC_CAP_EXTERNAL};

static BUTTPLUG_DATA_SIZE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/buttplug/data.bin.size"));
static BUTTPLUG_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/buttplug/data.bin.gz"));

#[derive(Debug, Deserialize)]
pub struct ButtplugData {
  base_communication_specifiers: HashMap<String, Vec<ProtocolCommunicationSpecifier>>,
  base_device_definitions:
    Vec<(ServerDeviceDefinition, Vec<BaseDeviceIdentifier>)>,
  base_device_definitions_count: usize,
}

impl ButtplugData {
  #[instrument]
  pub fn load() -> anyhow::Result<DeviceConfigurationManager> {
    #[cfg(target_os = "espidf")]
    let _guard = HEAP.use_caps(MALLOC_CAP_EXTERNAL);

    log::info!("Decompressing...");
    let size = postcard::from_bytes::<u32>(BUTTPLUG_DATA_SIZE)
      .map_err(|e| anyhow::anyhow!("Postcard deserialization error: {:?}", e))? as usize;
    let mut data = vec![0u8; size];
    miniz_oxide::inflate::decompress_slice_iter_to_slice(&mut data, core::iter::once(BUTTPLUG_DATA), false, true)
      .map_err(|_| anyhow::anyhow!("Decompression error!"))?;
    log::info!("Deserializing...");
    let Self {
      base_communication_specifiers,
      base_device_definitions: raw_base_device_definitions,
      base_device_definitions_count,
    } = postcard::from_bytes(&data)
      .map_err(|e| anyhow::anyhow!("Postcard deserialization error: {:?}", e))?;

    let mut base_device_definitions = HashMap::with_capacity(base_device_definitions_count);
    for (def, ids) in raw_base_device_definitions {
      let def = Arc::new(def);
      for id in ids {
        base_device_definitions.insert(id, def.clone());
      }
    }
    log::info!("Done building maps.");

    Ok(DeviceConfigurationManager::new(
      base_communication_specifiers,
      base_device_definitions,
    ))
  }
}
