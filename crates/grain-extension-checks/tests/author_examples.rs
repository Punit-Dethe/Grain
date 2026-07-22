use std::fs;
use std::path::PathBuf;

use grain_extension_checks::doctor;
use grain_sdk::GrainPack;

fn examples_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/Extension Platform/examples")
}

#[test]
fn checked_in_data_pack_is_valid() {
    let raw = fs::read_to_string(examples_root().join("prompt-pack.grainpack")).unwrap();
    let pack: GrainPack = serde_json::from_str(&raw).unwrap();
    pack.validate().unwrap();
}

#[test]
fn checked_in_scripted_examples_pass_doctor() {
    for name in ["click-counter", "workspace-surface", "voice-note"] {
        let report = doctor(&examples_root().join(name));
        assert!(report.is_clean(), "{name}: {report}");
    }
}
