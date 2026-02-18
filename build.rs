use std::collections::HashMap;

use buttplug_server_device_config::{ProtocolCommunicationSpecifier, load_protocol_configs};

fn main() {
  let out_dir = std::env::var("OUT_DIR").unwrap();
  let out_path = std::path::PathBuf::from(&out_dir);

  // OUT_DIR is target/<triple>/<profile>/build/<crate-hash>/out
  // We go up 3 levels to reach target/<triple>/<profile>/
  let bin_dir = out_path.ancestors().nth(3).unwrap();

  embuild::espidf::sysenv::output();

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

  std::fs::write(
    [&out_dir, "base_communication_specifiers.postcard.bin"]
      .iter()
      .collect::<std::path::PathBuf>(),
    postcard::to_stdvec(&base_communication_specifiers)?,
  )?;

  let base_device_definitions: HashMap<_, _> = dcm
    .base_device_definitions()
    .iter()
    .filter_map(|(identifier, definition)| Some((identifier.clone(), definition.clone())))
    .collect();

  std::fs::write(
    [&out_dir, "base_device_definitions.postcard.bin"]
      .iter()
      .collect::<std::path::PathBuf>(),
    postcard::to_stdvec(&base_device_definitions)?,
  )?;

  Ok(())
}
