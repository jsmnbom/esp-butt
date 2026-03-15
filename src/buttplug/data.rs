use std::{collections::HashMap, sync::Arc};

use buttplug_server_device_config::{
  BaseDeviceIdentifier,
  DeviceConfigurationManagerBuilder,
  ProtocolCommunicationSpecifier,
  ServerDeviceDefinition,
};
use compact_str::CompactString;
use serde::{Deserialize, de::DeserializeSeed};
use serde_describe::{DescribedBy, Schema, SelfDescribed};

static BUTTPLUG_DATA: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/buttplug/data.bin.gz"));

#[derive(Debug, Deserialize)]
pub struct ButtplugData {
  base_communication_specifiers: HashMap<CompactString, Vec<ProtocolCommunicationSpecifier>>,
  server_device_definition_schema: Schema,
  base_device_definitions: Vec<(Vec<u8>, Vec<BaseDeviceIdentifier>)>,
  base_device_definitions_count: usize,
}

impl ButtplugData {
  pub fn load() -> anyhow::Result<DeviceConfigurationManagerBuilder> {
    let data = miniz_oxide::inflate::decompress_to_vec(BUTTPLUG_DATA)
      .map_err(|_| anyhow::anyhow!("Decompression error!"))?;
    let data: Self = postcard::from_bytes(&data)
      .map_err(|e| anyhow::anyhow!("Postcard deserialization error: {:?}", e))?;

    let base_communication_specifiers = data.base_communication_specifiers;
    let mut base_device_definitions = HashMap::with_capacity(data.base_device_definitions_count);
    for (def, ids) in data.base_device_definitions {
      let DescribedBy(def, _) = data.server_device_definition_schema.describe_type::<ServerDeviceDefinition>().deserialize(&mut postcard::Deserializer::from_bytes(&def))?;

      let def = Arc::new(def);
      for id in ids {
        base_device_definitions.insert(id, def.clone());
      }
    }

    Ok(DeviceConfigurationManagerBuilder::new(
      base_communication_specifiers,
      base_device_definitions,
    ))
  }
}
