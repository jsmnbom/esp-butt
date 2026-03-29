use std::{collections::HashMap, sync::Arc};

use buttplug_server_device_config::{
  BaseDeviceIdentifier,
  DeviceConfigurationManager,
  GetBaseDeviceDefinition,
  ProtocolCommunicationSpecifier,
  ServerDeviceDefinition,
  UserDeviceIdentifier,
};
use compact_str::CompactString;
use dashmap::DashMap;
use serde::Deserialize;
use serde_describe::SelfDescribed;
use tracing::instrument;

static BUTTPLUG_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/buttplug/data.bin.gz"));

#[derive(Debug, Deserialize)]
pub struct RawButtplugData {
  base_communication_specifiers: HashMap<CompactString, Vec<ProtocolCommunicationSpecifier>>,
  base_device_definitions: Vec<(Vec<BaseDeviceIdentifier>, Vec<u8>)>,
  base_device_definitions_count: usize,
}

enum DeviceDefinition {
  Unloaded(Vec<u8>),
  Loaded(Arc<ServerDeviceDefinition>),
}

pub struct ButtplugData {
  base_device_definitions: DashMap<BaseDeviceIdentifier, Arc<DeviceDefinition>>,
}

impl ButtplugData {
  #[instrument]
  pub fn load() -> anyhow::Result<DeviceConfigurationManager> {
    log::info!("Decompressing...");
    let data = miniz_oxide::inflate::decompress_to_vec(BUTTPLUG_DATA)
      .map_err(|_| anyhow::anyhow!("Decompression error!"))?;
    log::info!("Deserializing...");
    let raw: RawButtplugData = postcard::from_bytes(&data)
      .map_err(|e| anyhow::anyhow!("Postcard deserialization error: {:?}", e))?;

    // log::info!("Building device configuration manager...");
    let base_communication_specifiers = raw.base_communication_specifiers.clone();

    log::info!(
      "Raw data loaded: {} communication specifiers, {} device definitions ({} total identifiers)",
      raw.base_communication_specifiers.len(),
      raw.base_device_definitions.len(),
      raw.base_device_definitions_count
    );

    let base_device_definitions = DashMap::with_capacity(raw.base_device_definitions_count);
    for (identifiers, raw_def) in raw.base_device_definitions {
      let def = Arc::new(DeviceDefinition::Unloaded(raw_def));
      for identifier in identifiers {
        base_device_definitions.insert(identifier, def.clone());
      }
    }

    let data = ButtplugData {
      base_device_definitions,
    };

    Ok(DeviceConfigurationManager::new(
      base_communication_specifiers,
      HashMap::new(),
      Some(Box::new(data)),
    ))
  }
}

impl GetBaseDeviceDefinition for ButtplugData {
  fn get_base_device_definition(
    &self,
    identifier: &UserDeviceIdentifier,
  ) -> Option<Arc<ServerDeviceDefinition>> {
    if let Some(mut entry) = self
      .base_device_definitions
      .get_mut(&BaseDeviceIdentifier::new(
        identifier.protocol(),
        identifier.identifier(),
      ))
    {
      match &**entry.value_mut() {
        DeviceDefinition::Loaded(def) => Some(def.clone()),
        DeviceDefinition::Unloaded(raw) => {
          let wrapped_loaded: SelfDescribed<ServerDeviceDefinition> =
            postcard::from_bytes(raw).unwrap();
          let loaded = Arc::new(wrapped_loaded.0);
          *entry = Arc::new(DeviceDefinition::Loaded(loaded.clone()));
          Some(loaded)
        }
      }
    } else if let Some(mut entry) = self
      .base_device_definitions
      .get_mut(&BaseDeviceIdentifier::new_default(identifier.protocol()))
    {
      match &**entry.value_mut() {
        DeviceDefinition::Loaded(def) => Some(def.clone()),
        DeviceDefinition::Unloaded(raw) => {
          let wrapped_loaded: SelfDescribed<ServerDeviceDefinition> =
            postcard::from_bytes(raw).unwrap();
          let loaded = Arc::new(wrapped_loaded.0);
          *entry = Arc::new(DeviceDefinition::Loaded(loaded.clone()));
          Some(loaded)
        }
      }
    } else {
      None
    }
  }
}
