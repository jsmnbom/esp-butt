use buttplug_server_device_config::{ProtocolCommunicationSpecifier, load_protocol_configs};
use std::collections::HashMap;

fn main() {
    embuild::espidf::sysenv::output();

    println!("cargo::rustc-check-cfg=cfg(esp32)");
  println!("cargo::rustc-check-cfg=cfg(esp32c3)");

  println!("cargo::rustc-check-cfg=cfg(esp_idf_soc_esp_nimble_controller)");
  println!("cargo::rustc-check-cfg=cfg(esp_idf_bt_nimble_ext_adv)");

  println!("cargo::rustc-check-cfg=cfg(esp_idf_version_major, values(\"4\", \"5\"))");
  println!("cargo::rustc-check-cfg=cfg(esp_idf_version_minor, values(\"2\", \"3\", \"4\", \"5\"))");
  println!("cargo::rustc-check-cfg=cfg(esp_idf_version_patch, values(\"0\"))");
  println!("cargo::rustc-check-cfg=cfg(esp_idf_version_patch, values(\"1\"))");
  println!("cargo::rustc-check-cfg=cfg(esp_idf_version_patch, values(\"2\"))");

   println!("cargo::rustc-check-cfg=cfg(cpfd)");
   println!("cargo::rustc-cfg=cpfd");
   

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
