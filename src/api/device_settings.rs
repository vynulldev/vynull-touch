// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::device_settings::{Change, Snapshot};
use serde_json::{Value, json};

impl Api {
    pub(super) fn device_settings(&self) -> Result<Snapshot, String> {
        let response: Value = self.get("api/settings")?;
        let snapshot = Snapshot {
            full: response["full"].clone(),
            enums: self.get("api/settings/enums")?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(super) fn save_device_settings(&self, changes: &[Change]) -> Result<Snapshot, String> {
        if changes.is_empty() {
            return self.device_settings();
        }
        // The existing endpoint replaces the entire profile. Merge against a
        // fresh read, rejecting overlapping edits and preserving other values.
        // This is not an atomic CAS: the daemon remains last-write-wins.
        let latest = self.device_settings()?;
        let full = latest.merge(changes)?;
        let result = self.json::<Value>(
            Method::POST,
            "api/settings",
            Some(json!({"full": full})),
            10,
        );
        if let Err(error) = result {
            return Err(format!(
                "Save was not confirmed; reload before retrying. {error}"
            ));
        }
        let saved = self.device_settings().map_err(|error| {
            format!("Settings were sent, but could not be read back. Reload to check them. {error}")
        })?;
        saved.verify(changes)?;
        Ok(saved)
    }
}

#[cfg(test)]
mod tests;
