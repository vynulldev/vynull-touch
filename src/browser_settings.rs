// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Category {
    pub key: String,
    pub label: String,
    pub visible: bool,
    pub locked: bool,
    #[serde(default)]
    pub export_only: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Sort {
    pub id: u32,
    pub label: String,
    pub visible: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DetailField {
    pub key: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Snapshot {
    pub items: Vec<Category>,
    pub sorts: Vec<Sort>,
    pub track_detail: String,
    pub track_detail_fields: Vec<DetailField>,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub before: Snapshot,
    pub draft: Snapshot,
}

impl Snapshot {
    pub fn validate(&self) -> Result<(), String> {
        let categories: HashSet<_> = self.items.iter().map(|v| &v.key).collect();
        let sorts: HashSet<_> = self.sorts.iter().map(|v| v.id).collect();
        let fields: HashSet<_> = self.track_detail_fields.iter().map(|v| &v.key).collect();
        if self.items.is_empty()
            || self.items.len() > 256
            || categories.len() != self.items.len()
            || self
                .items
                .iter()
                .any(|v| v.key.is_empty() || v.label.is_empty() || (v.locked && !v.visible))
            || self.sorts.is_empty()
            || self.sorts.len() > 256
            || sorts.len() != self.sorts.len()
            || self.sorts.iter().any(|v| v.label.is_empty())
            || !self.sorts.iter().any(|v| v.visible)
            || self.track_detail_fields.is_empty()
            || self.track_detail_fields.len() > 256
            || fields.len() != self.track_detail_fields.len()
            || self
                .track_detail_fields
                .iter()
                .any(|v| v.key.is_empty() || v.label.is_empty())
            || !fields.contains(&self.track_detail)
        {
            return Err(
                "Incomplete or invalid CDJ browser settings. Reload or update Vynull.".into(),
            );
        }
        Ok(())
    }

    pub fn same_categories(&self, other: &Self) -> bool {
        self.items
            .iter()
            .map(|v| (&v.key, v.visible, v.locked, v.export_only))
            .eq(other
                .items
                .iter()
                .map(|v| (&v.key, v.visible, v.locked, v.export_only)))
    }

    pub fn same_sorts(&self, other: &Self) -> bool {
        self.sorts
            .iter()
            .map(|v| (v.id, v.visible))
            .eq(other.sorts.iter().map(|v| (v.id, v.visible)))
    }

    pub fn same_config(&self, other: &Self) -> bool {
        self.same_categories(other)
            && self.same_sorts(other)
            && self.track_detail == other.track_detail
    }

    pub fn toggle_category(&mut self, key: &str) {
        if let Some(item) = self.items.iter_mut().find(|v| v.key == key && !v.locked) {
            item.visible = !item.visible;
        }
    }

    pub fn toggle_sort(&mut self, id: u32) {
        let shown = self.sorts.iter().filter(|v| v.visible).count();
        if let Some(item) = self
            .sorts
            .iter_mut()
            .find(|v| v.id == id && (!v.visible || shown > 1))
        {
            item.visible = !item.visible;
        }
    }

    /// Move through the visible menu; hidden entries do not consume a position.
    pub fn move_category(&mut self, key: &str, delta: isize) {
        let shown: Vec<_> = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, v)| v.visible)
            .map(|(i, _)| i)
            .collect();
        if let Some(index) = self.items.iter().position(|v| v.key == key) {
            swap_visible(&mut self.items, &shown, index, delta);
        }
    }

    pub fn move_sort(&mut self, id: u32, delta: isize) {
        let shown: Vec<_> = self
            .sorts
            .iter()
            .enumerate()
            .filter(|(_, v)| v.visible)
            .map(|(i, _)| i)
            .collect();
        if let Some(index) = self.sorts.iter().position(|v| v.id == id) {
            swap_visible(&mut self.sorts, &shown, index, delta);
        }
    }

    pub fn demo() -> Self {
        serde_json::from_str(include_str!("browser_settings/demo.json"))
            .expect("bundled browser fixture")
    }
}

fn swap_visible<T>(items: &mut [T], shown: &[usize], index: usize, delta: isize) {
    if !matches!(delta, -1 | 1) {
        return;
    }
    if let Some(at) = shown.iter().position(|i| *i == index)
        && let Some(next) = at.checked_add_signed(delta).and_then(|i| shown.get(i))
    {
        items.swap(index, *next);
    }
}

impl Edit {
    pub fn prepare(&self, latest: &Snapshot) -> Result<(Value, Snapshot), String> {
        self.before.validate()?;
        self.draft.validate()?;
        latest.validate()?;
        let mut body = json!({});
        let mut target = latest.clone();
        if !self.before.same_categories(&self.draft) {
            if !self.before.same_categories(latest) {
                return Err("Categories changed on Vynull. Reload before saving.".into());
            }
            // Only order and visibility are editable. Labels and capabilities
            // always come from the fresh response, including custom labels.
            target.items = self
                .draft
                .items
                .iter()
                .map(|item| {
                    let mut fresh = latest
                        .items
                        .iter()
                        .find(|v| v.key == item.key)
                        .ok_or("Category list changed. Reload before saving.")?
                        .clone();
                    if fresh.locked && !item.visible {
                        return Err("Required categories must stay shown.");
                    }
                    fresh.visible = item.visible;
                    Ok(fresh)
                })
                .collect::<Result<_, &str>>()?;
            if target.items.len() != latest.items.len() {
                return Err("Category list changed. Reload before saving.".into());
            }
            body["items"] = json!(
                target
                    .items
                    .iter()
                    .map(|v| json!({"key":v.key,"label":v.label,"visible":v.visible}))
                    .collect::<Vec<_>>()
            );
        }
        if !self.before.same_sorts(&self.draft) {
            if !self.before.same_sorts(latest) {
                return Err("Sort options changed on Vynull. Reload before saving.".into());
            }
            target.sorts = self
                .draft
                .sorts
                .iter()
                .map(|item| {
                    let mut fresh = latest
                        .sorts
                        .iter()
                        .find(|v| v.id == item.id)
                        .ok_or("Sort options changed. Reload before saving.")?
                        .clone();
                    fresh.visible = item.visible;
                    Ok(fresh)
                })
                .collect::<Result<_, &str>>()?;
            if target.sorts.len() != latest.sorts.len() {
                return Err("Sort options changed. Reload before saving.".into());
            }
            body["sorts"] = json!(
                target
                    .sorts
                    .iter()
                    .map(|v| json!({"id":v.id,"visible":v.visible}))
                    .collect::<Vec<_>>()
            );
        }
        if self.before.track_detail != self.draft.track_detail {
            if latest.track_detail != self.before.track_detail {
                return Err("Track detail changed on Vynull. Reload before saving.".into());
            }
            if !latest
                .track_detail_fields
                .iter()
                .any(|f| f.key == self.draft.track_detail)
            {
                return Err(
                    "That track-detail field is no longer available. Reload settings.".into(),
                );
            }
            target.track_detail.clone_from(&self.draft.track_detail);
            body["track_detail"] = json!(target.track_detail);
        }
        target.validate()?;
        Ok((body, target))
    }

    pub fn verify(&self, target: &Snapshot, saved: &Snapshot) -> Result<(), String> {
        if (!self.before.same_categories(&self.draft) && !target.same_categories(saved))
            || (!self.before.same_sorts(&self.draft) && !target.same_sorts(saved))
            || (self.before.track_detail != self.draft.track_detail
                && target.track_detail != saved.track_detail)
        {
            return Err(
                "Browser settings did not match when read back. Reload to review the saved values."
                    .into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
