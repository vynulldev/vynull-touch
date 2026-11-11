// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::browser_settings::{Edit, Snapshot};
use serde_json::Value;

impl Api {
    pub(super) fn browser_settings(&self) -> Result<Snapshot, String> {
        let snapshot: Snapshot = self.get("api/menu-items")?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub(super) fn save_browser_settings(&self, edit: &Edit) -> Result<Snapshot, String> {
        let latest = self.browser_settings()?;
        let (body, target) = edit.prepare(&latest)?;
        if body.as_object().is_none_or(|v| v.is_empty()) {
            return Ok(latest);
        }
        // PUT applies its components sequentially; failures may be partial.
        // Preflight detects existing conflicts but is not an atomic CAS.
        self.json::<Value>(Method::PUT, "api/menu-items", Some(body), 10).map_err(|error| {
            format!("Save was not confirmed; some changes may have applied. Reload before retrying. {error}")
        })?;
        let saved = self.browser_settings().map_err(|error| {
            format!("Browser settings were sent but could not be read back. Reload to check them. {error}")
        })?;
        edit.verify(&target, &saved)?;
        Ok(saved)
    }
}

#[cfg(test)]
mod tests;
