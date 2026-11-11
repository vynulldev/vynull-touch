// SPDX-License-Identifier: GPL-3.0-or-later
//! The daemon owns this shared hardware profile. Keep the complete JSON so
//! updating one field does not reset settings unknown to this client.
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const SECTIONS: [&str; 4] = [
    "my_setting",
    "my_setting_2",
    "dev_setting",
    "djm_my_setting",
];

#[derive(Clone, Debug, Deserialize)]
pub struct Snapshot {
    pub full: Value,
    pub enums: BTreeMap<String, Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Change {
    pub section: String,
    pub field: String,
    pub before: String,
    pub after: String,
}

impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        if !self.full.is_object()
            || SECTIONS
                .iter()
                .any(|section| !self.full[section].is_object())
            || self.enums.is_empty()
        {
            return Err("This server does not provide complete CDJ/mixer settings. Update Vynull and reload.".into());
        }
        Ok(())
    }

    pub fn changes(&self, draft: &Value) -> Result<Vec<Change>, String> {
        let mut changes = Vec::new();
        for section in SECTIONS {
            let fields = self.full[section]
                .as_object()
                .ok_or("Missing settings section")?;
            for (field, before) in fields {
                let after = &draft[section][field];
                if after == before {
                    continue;
                }
                let before = before.as_str().ok_or("Unsupported setting type")?;
                let after = after.as_str().ok_or("Unsupported setting type")?;
                self.validate_choice(section, field, after)?;
                changes.push(Change {
                    section: section.into(),
                    field: field.clone(),
                    before: before.into(),
                    after: after.into(),
                });
            }
        }
        Ok(changes)
    }

    fn validate_choice(&self, section: &str, field: &str, value: &str) -> Result<(), String> {
        if !SECTIONS.contains(&section)
            || !self.full[section][field].is_string()
            || !self
                .enums
                .get(field)
                .is_some_and(|choices| choices.iter().any(|c| c == value))
        {
            return Err(format!(
                "{} is no longer available. Reload settings.",
                label(field)
            ));
        }
        Ok(())
    }

    pub fn merge(&self, changes: &[Change]) -> Result<Value, String> {
        self.validate()?;
        let mut full = self.full.clone();
        for change in changes {
            self.validate_choice(&change.section, &change.field, &change.after)?;
            if self.full[&change.section][&change.field].as_str() != Some(&change.before) {
                return Err(format!(
                    "{} changed on the server. Reload settings before saving.",
                    label(&change.field)
                ));
            }
            full[&change.section][&change.field] = Value::String(change.after.clone());
        }
        Ok(full)
    }

    pub fn verify(&self, changes: &[Change]) -> Result<(), String> {
        if let Some(change) = changes
            .iter()
            .find(|c| self.full[&c.section][&c.field].as_str() != Some(&c.after))
        {
            return Err(format!(
                "{} did not match when read back. Reload settings to check the saved values.",
                label(&change.field)
            ));
        }
        Ok(())
    }

    pub fn demo() -> Self {
        // Fictional profile using the daemon's published defaults and enum schema.
        serde_json::from_str(include_str!("device_settings/demo.json"))
            .expect("bundled settings fixture")
    }
}

pub fn label(key: &str) -> String {
    match key {
        "lcd_brightness" => "LCD brightness".into(),
        "jog_lcd_brightness" => "Jog LCD brightness".into(),
        "hotcue_autoload" => "Hot cue auto load".into(),
        "hotcue_color" => "Hot cue colors".into(),
        "quantize_beat_value" => "Quantize size".into(),
        "beat_jump_beat_value" => "Beat jump size".into(),
        "wave_position" => "Playhead position".into(),
        "overview_type" => "Full-track waveform".into(),
        "headphones_pre_eq" => "Headphone EQ".into(),
        "headphones_mono_split" => "Headphone mode".into(),
        "beat_fx_quantize" => "Beat FX quantize".into(),
        "midi_channel" => "MIDI channel".into(),
        "midi_button_type" => "MIDI button type".into(),
        "channel_fader_curve_long" => "Long channel fader curve".into(),
        _ => {
            let text = key.replace('_', " ");
            let mut chars = text.chars();
            chars.next().map_or_else(String::new, |first| {
                first.to_uppercase().to_string() + chars.as_str()
            })
        }
    }
}

pub fn choice_label(field: &str, value: &str) -> String {
    let number = match value {
        "eighth" => Some("1/8"),
        "quarter" => Some("1/4"),
        "half" => Some("1/2"),
        "one" => Some("1"),
        "two" => Some("2"),
        "three" => Some("3"),
        "four" => Some("4"),
        "five" => Some("5"),
        "six" => Some("6"),
        "seven" => Some("7"),
        "eight" => Some("8"),
        "nine" => Some("9"),
        "ten" => Some("10"),
        "eleven" => Some("11"),
        "twelve" => Some("12"),
        "thirteen" => Some("13"),
        "fourteen" => Some("14"),
        "fifteen" => Some("15"),
        "sixteen" => Some("16"),
        "thirty_two" => Some("32"),
        "sixty_four" => Some("64"),
        _ => None,
    };
    if let Some(number) = number.filter(|_| !(field == "overview_type" && value == "half")) {
        return match field {
            "tempo_range" => format!("±{number}%"),
            "quantize_beat_value" | "beat_jump_beat_value" => {
                format!("{number} beat{}", if value == "one" { "" } else { "s" })
            }
            _ => number.into(),
        };
    }
    if let Some(db) = value
        .strip_prefix("minus_")
        .and_then(|s| s.strip_suffix("db"))
    {
        return format!("−{db} dB");
    }
    match value {
        "rgb" => "RGB".into(),
        "3band" => "3-band".into(),
        "cdj" => "CDJ".into(),
        "post_eq" => "Post EQ".into(),
        "pre_eq" => "Pre EQ".into(),
        "type1" => "Type 1".into(),
        "type2" => "Type 2".into(),
        "remain" => "Remaining".into(),
        "rekordbox_setting" => "rekordbox setting".into(),
        "classic" => "Classic (Am)".into(),
        "alphanumeric" => "Alphanumeric (8A)".into(),
        _ if value.starts_with("0x") => format!("Device value {value}"),
        _ => label(value),
    }
}

#[cfg(test)]
mod tests;
