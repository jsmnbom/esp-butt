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

  // Parallel symbol resolution in GNU ld
  // println!("cargo:rustc-link-arg=-Wl,--threads");

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

  use buttplug_server_device_config::{
    BaseDeviceIdentifier,
    ProtocolCommunicationSpecifier,
    ServerDeviceDefinition,
    load_protocol_configs,
  };
  use compact_str::CompactString;
  use serde::Serialize;
  use serde_describe::{Schema, SchemaBuilder};

  #[derive(Serialize)]
  pub struct ButtplugData {
    // We hope none of ProtocolCommunicationSpecifier needs SelfDescribed :P
    base_communication_specifiers: HashMap<CompactString, Vec<ProtocolCommunicationSpecifier>>,
    // Reverse map of ServerDeviceDefinition to its identifiers
    server_device_definition_schema: Schema,
    base_device_definitions: Vec<(Vec<u8>, Vec<BaseDeviceIdentifier>)>,
    // Length of base_device_definitions when unpacked - aka total count of identifiers - used to preallocate the HashMap when loading
    base_device_definitions_count: usize,
  }

  impl ButtplugData {
    pub fn build() -> anyhow::Result<Vec<u8>> {
      let dcm = load_protocol_configs(&None, &None, false)?.finish()?;

      let mut base_device_definitions_groups: HashMap<
        *const ServerDeviceDefinition,
        (ServerDeviceDefinition, Vec<BaseDeviceIdentifier>),
      > = HashMap::new();

      for (identifier, definition) in dcm.base_device_definitions() {
        base_device_definitions_groups
          .entry(Arc::as_ptr(definition))
          .or_insert_with(|| (definition.deref().clone(), Vec::new()))
          .1
          .push(identifier.clone());
      }

      let mut schema_builder = SchemaBuilder::new();
      let base_device_definitions_traces = base_device_definitions_groups
        .into_values()
        .map(|(definition, identifiers)| -> anyhow::Result<_> {
          let trace = schema_builder.trace(&definition)?;
          Ok((trace, identifiers))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

      let server_device_definition_schema = schema_builder.build()?;

      let base_device_definitions = base_device_definitions_traces
        .into_iter()
        .map(|(definition, identifiers)| -> anyhow::Result<_> {
          Ok((
            postcard::to_allocvec(&server_device_definition_schema.describe_trace(definition))?,
            identifiers,
          ))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

      let data = Self {
        base_communication_specifiers: dcm.base_communication_specifiers().clone(),
        server_device_definition_schema,
        base_device_definitions,
        base_device_definitions_count: dcm.base_device_definitions().len(),
      };

      let out = postcard::to_allocvec(&data)?;
      Ok(out)
    }
  }

  pub fn generate(out_dir: PathBuf) -> anyhow::Result<()> {
    std::fs::create_dir_all(&out_dir)?;
    let out_dir = Path::new(&out_dir).canonicalize()?;

    let data = ButtplugData::build()?;
    std::fs::write(
      out_dir.join("data.bin.gz"),
      miniz_oxide::deflate::compress_to_vec(&data, 6),
    )?;

    Ok(())
  }
}
