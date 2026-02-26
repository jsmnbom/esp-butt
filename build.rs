use std::collections::HashMap;

use buttplug_server_device_config::{ProtocolCommunicationSpecifier, load_protocol_configs};
use miniz_oxide::deflate::compress_to_vec;
use rustc_hash::FxBuildHasher;

fn main() {
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let out_path = std::path::PathBuf::from(&out_dir);

  if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "espidf" {
    embuild::espidf::sysenv::output();
  }

  // OUT_DIR is target/<triple>/<profile>/build/<crate-hash>/out
  // We go up 3 levels to reach target/<triple>/<profile>/
  let bin_dir = out_path.ancestors().nth(3).unwrap();

  println!(
    "cargo:rustc-link-arg=-Wl,-Map={}/linker.map",
    bin_dir.display()
  );

  if let Err(e) = generate_buttplug_data() {
    panic!("Failed to generate Buttplug data: {e}");
  }
}

fn generate_buttplug_data() -> anyhow::Result<()> {
  let out_dir = "./src/buttplug/data/";

  std::fs::create_dir_all(&out_dir)?;

  let dcm = load_protocol_configs(&None, &None, false)?.finish()?;

  let base_communication_specifiers: Vec<_> = dcm
    .base_communication_specifiers()
    .iter()
    .filter_map(|(protocol_name, specs)| {
      let specs: Vec<_> = specs
        .iter()
        .cloned()
        .filter(|spec| match spec {
          ProtocolCommunicationSpecifier::BluetoothLE(_) => true,
          _ => false,
        })
        .collect();
      if specs.is_empty() {
        None
      } else {
        Some((protocol_name.clone(), specs))
      }
    })
    .collect();

  let base_communication_specifiers_postcard = postcard::to_stdvec(&base_communication_specifiers)?;

  std::fs::write(
    [&out_dir, "base_communication_specifiers.postcard.bin"]
      .iter()
      .collect::<std::path::PathBuf>(),
    &base_communication_specifiers_postcard,
  )?;

  std::fs::write(
    [&out_dir, "base_communication_specifiers.postcard.bin.gz"]
      .iter()
      .collect::<std::path::PathBuf>(),
    compress_to_vec(&base_communication_specifiers_postcard, 6),
  )?;

  let base_device_definitions: HashMap<_, _, FxBuildHasher> = dcm
    .base_device_definitions()
    .iter()
    .filter_map(|(identifier, definition)| Some((identifier.clone(), definition.clone())))
    .collect();

  let base_device_definitions_postcard = postcard::to_stdvec(&base_device_definitions)?;

  std::fs::write(
    [&out_dir, "base_device_definitions.postcard.bin"]
      .iter()
      .collect::<std::path::PathBuf>(),
    &base_device_definitions_postcard,
  )?;

  std::fs::write(
    [&out_dir, "base_device_definitions.postcard.bin.gz"]
      .iter()
      .collect::<std::path::PathBuf>(),
    compress_to_vec(&base_device_definitions_postcard, 6),
  )?;

  Ok(())
}
