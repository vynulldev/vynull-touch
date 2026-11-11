// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde_json::json;

#[test]
fn all_hardware_fields_accept_server_choices_and_preserve_other_config() {
    let snapshot = Snapshot::demo();
    snapshot.validate().unwrap();
    let mut count = 0;
    for section in SECTIONS {
        for (field, before) in snapshot.full[section].as_object().unwrap() {
            for after in &snapshot.enums[field] {
                let mut draft = snapshot.full.clone();
                draft[section][field] = json!(after);
                let changes = snapshot.changes(&draft).unwrap();
                let mut fresh = snapshot.clone();
                fresh.full["grid_offset_ms"] = json!(-23.5);
                fresh.full["bpm_range_min"] = json!(90);
                fresh.full["future"] = json!({"preserve":true});
                let merged = fresh.merge(&changes).unwrap();
                assert_eq!(merged[section][field], *after);
                assert_eq!(merged["grid_offset_ms"], -23.5);
                assert_eq!(merged["bpm_range_min"], 90);
                assert_eq!(merged["future"]["preserve"], true);
                assert_eq!(changes.is_empty(), before == after);
            }
            count += 1;
        }
    }
    assert_eq!(count, 46);
}

#[test]
fn stale_fields_invalid_choices_and_missing_sections_cannot_be_written() {
    let original = Snapshot::demo();
    let mut draft = original.full.clone();
    draft["dev_setting"]["waveform_color"] = json!("blue");
    let changes = original.changes(&draft).unwrap();
    let mut fresh = original.clone();
    fresh.full["dev_setting"]["waveform_color"] = json!("3band");
    assert!(
        fresh
            .merge(&changes)
            .unwrap_err()
            .contains("changed on the server")
    );
    fresh = original.clone();
    fresh
        .enums
        .get_mut("waveform_color")
        .unwrap()
        .retain(|s| s != "blue");
    assert!(fresh.merge(&changes).is_err());
    draft["dev_setting"]["waveform_color"] = json!("invalid");
    assert!(original.changes(&draft).is_err());
    fresh.full["my_setting"] = Value::Null;
    assert!(fresh.validate().is_err());
}

#[test]
fn unknown_device_values_are_preserved_until_explicitly_changed() {
    let mut snapshot = Snapshot::demo();
    snapshot.full["my_setting"]["jog_mode"] = json!("0x92");
    snapshot.full["my_setting"]["future_option"] = json!("future_value");
    let mut draft = snapshot.full.clone();
    draft["my_setting"]["auto_cue"] = json!("off");
    let merged = snapshot.merge(&snapshot.changes(&draft).unwrap()).unwrap();
    assert_eq!(merged["my_setting"]["jog_mode"], "0x92");
    assert_eq!(merged["my_setting"]["future_option"], "future_value");
    assert_eq!(choice_label("jog_mode", "0x92"), "Device value 0x92");
    assert_eq!(choice_label("tempo_range", "sixteen"), "±16%");
    assert_eq!(choice_label("quantize_beat_value", "quarter"), "1/4 beats");
    assert_eq!(choice_label("overview_type", "half"), "Half");
}
