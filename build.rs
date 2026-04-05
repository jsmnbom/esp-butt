use std::path::PathBuf;

fn main() {
  if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "espidf" {
    embuild::espidf::sysenv::output();
  }

  let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());

  // Generate linker map file path for inspecting with esp_idf_size
  // OUT_DIR is target/<triple>/<profile>/build/<crate-hash>/out
  // We go up 3 levels to reach target/<triple>/<profile>/
  println!(
    "cargo:rustc-link-arg=-Wl,-Map={}/linker.map",
    out.ancestors().nth(3).unwrap().display()
  );

  if let Err(e) = img_data::generate(PathBuf::from("./img"), out.join("img")) {
    panic!("Failed to generate image data: {e}");
  }

  if let Err(e) = buttplug_data::generate(out.join("buttplug")) {
    panic!("Failed to generate Buttplug data: {e}");
  }
}

mod img_data {
  use std::{fs, path::PathBuf};

  pub fn generate(img_dir: PathBuf, out_dir: PathBuf) -> anyhow::Result<()> {
    let on = image::Rgb([255, 255, 255]);

    fs::create_dir_all(&out_dir)?;
    let img_dir = img_dir.canonicalize()?;
    let out_dir = out_dir.canonicalize()?;

    let mut images = Vec::<(String, u32)>::new();

    for entry in fs::read_dir(img_dir)? {
      let entry = entry?;
      let path = entry.path();
      if path.is_file() {
        let out_path = out_dir
          .join(path.file_name().unwrap())
          .with_extension("bin");
        let img = image::open(&path)?.to_rgb8();
        let row_width_bytes = (img.width() as usize + 7) / 8;
        let mut out = vec![0; img.height() as usize * row_width_bytes];

        // Turn into 1bpp data, row by row
        // Each row needs to be padded to a byte boundary
        for y in 0..img.height() {
          for x in 0..img.width() {
            let pixel = img.get_pixel(x, y);
            let bit = if *pixel == on { 1 } else { 0 };
            let byte_index = (y as usize * row_width_bytes) + (x as usize / 8);
            out[byte_index] |= bit << (7 - (x % 8));
          }
        }
        images.push((
          out_path.file_name().unwrap().to_string_lossy().to_string(),
          img.width(),
        ));
        fs::write(out_path, &out)?;
      }
    }
    let code = images
      .iter()
      .map(|(name, width)| format!("#[rustfmt::skip]\npub const {}: ::embedded_graphics::image::ImageRaw<embedded_graphics::pixelcolor::BinaryColor> = ::embedded_graphics::image::ImageRaw::new(include_bytes!(\"./{}\"), {});", name.trim_end_matches(".bin").to_uppercase(), name, width))
      .collect::<Vec<_>>()
      .join("\n");

    fs::write(out_dir.join("mod.rs"), code)?;

    Ok(())
  }
}

mod buttplug_data {
  use std::{
    collections::HashMap,
    ops::Deref,
    path::{Path, PathBuf},
    sync::Arc,
  };

  use buttplug_core::message::{InputType, OutputType};
  use buttplug_server_device_config::{
    BaseDeviceIdentifier,
    ProtocolCommunicationSpecifier,
    ServerDeviceDefinition,
    ServerDeviceDefinitionBuilder,
    load_protocol_configs,
  };
  use serde::Serialize;

  #[derive(Serialize)]
  pub struct ButtplugData {
    base_communication_specifiers: HashMap<String, Vec<ProtocolCommunicationSpecifier>>,
    // Reverse map of ServerDeviceDefinition to its identifiers
    base_device_definitions: Vec<(ServerDeviceDefinition, Vec<BaseDeviceIdentifier>)>,
    // Length of base_device_definitions when unpacked - aka total count of identifiers - used to preallocate the HashMap when loading
    base_device_definitions_count: usize,
  }

  impl ButtplugData {
    pub fn build() -> anyhow::Result<Self> {
      let dcm = load_protocol_configs(&None, &None, false)?.finish()?;

      // Filter communication specifiers to only include those with BluetoothLE.
      let base_communication_specifiers = dcm
        .base_communication_specifiers()
        .iter()
        .filter_map(|(k, v)| {
          if v
            .iter()
            .any(|specifier| matches!(specifier, ProtocolCommunicationSpecifier::BluetoothLE(..)))
          {
            Some((k.clone(), v.clone()))
          } else {
            None
          }
        })
        .collect::<HashMap<_, _>>();

      // Group device definitions by pointer, filtering out those that don't have any valid communication specifiers (non BluetoothLE).
      let mut base_device_definitions_groups: HashMap<
        *const ServerDeviceDefinition,
        (ServerDeviceDefinition, Vec<BaseDeviceIdentifier>),
      > = HashMap::new();

      for (identifier, definition) in dcm.base_device_definitions() {
        if !base_communication_specifiers.contains_key(identifier.protocol().as_str()) {
          continue;
        }

        base_device_definitions_groups
          .entry(Arc::as_ptr(definition))
          .or_insert_with(|| (definition.deref().clone(), Vec::new()))
          .1
          .push(identifier.clone());
      }

      // Filter out any definitions that only support hw_position_with_duration outputs.
      // Filter out any inputs that aren't battery.

      let base_device_definitions = base_device_definitions_groups
        .into_values()
        .filter_map(|(def, ids)| {
          let mut builder = ServerDeviceDefinitionBuilder::new(def.name(), &def.id());
          if let Some(base_id) = def.base_id() {
            builder.base_id(base_id);
          }
          builder.display_name(def.display_name());
          if let Some(protocol_variant) = def.protocol_variant() {
            builder.protocol_variant(protocol_variant);
          }
          builder.message_gap_ms(def.message_gap_ms());

          builder.allow(def.allow());
          builder.deny(def.deny());
          builder.index(def.index());

          for (_, feature) in def.features() {
            let mut feature = feature.clone();
            feature.description = "".into(); // Strip descriptions to save space, as they aren't used.

            if feature
              .output
              .iter()
              .any(|output| output.output_type() != OutputType::HwPositionWithDuration)
            {
              builder.add_feature(&feature);
            } else if feature
              .input
              .iter()
              .any(|input| input.input_type() == InputType::Battery)
            {
              builder.add_feature(&feature);
            }
          }
          let def = builder.finish();
          if def.features().is_empty() {
            return None;
          }

          Some((def, ids))
        })
        .collect::<Vec<_>>();

      // Now filter out any communication specifiers that aren't used by any device definitions.
      let base_communication_specifiers = base_communication_specifiers
        .into_iter()
        .filter(|(protocol, _)| {
          base_device_definitions
            .iter()
            .any(|(_, ids)| ids.iter().any(|id| id.protocol() == protocol.as_str()))
        })
        .collect::<HashMap<_, _>>();

      // Finally count the total number of device definitions by counting the identifiers, so we can preallocate the HashMap when loading.
      let base_device_definitions_count = base_device_definitions
        .iter()
        .map(|(_, ids)| ids.len())
        .sum();

      Ok(Self {
        base_communication_specifiers,
        base_device_definitions,
        base_device_definitions_count,
      })
    }
  }

  pub fn generate(out_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&out_dir)?;
    let out_dir = Path::new(&out_dir).canonicalize()?;

    let data = ButtplugData::build()?;
    let out = postcard::to_allocvec(&data)?;
    std::fs::write(
      out_dir.join("data.bin.size"),
      postcard::to_allocvec(&(out.len() as u32))?,
    )?;
    std::fs::write(out_dir.join("data.bin"), &out)?;
    std::fs::write(
      out_dir.join("data.json"),
      serde_json::to_string_pretty(&data)?,
    )?;
    std::fs::write(
      out_dir.join("data.bin.gz"),
      miniz_oxide::deflate::compress_to_vec(&out, 6),
    )?;

    Ok(())
  }
}
