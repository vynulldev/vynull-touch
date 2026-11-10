// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::editing::{self, Catalog, Category, Reply, Request, Tag, TrackSnapshot};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[cfg(test)]
mod tests;

impl Api {
    pub(super) fn edit(&self, request: &Request) -> Result<Reply, String> {
        if request.writes() && self.session.is_none() {
            return Err(
                "Reconnect to a server with library identity support before editing".into(),
            );
        }
        match request {
            Request::Track(track) => self
                .edit_track_snapshot(track)
                .map(|s| Reply::Track(Box::new(s))),
            Request::SaveTrack {
                before,
                rating,
                color,
                tags,
            } => self.save_track(before, *rating, *color, tags),
            Request::Catalog => Ok(Reply::Catalog(self.tag_catalog()?, None)),
            Request::Tag {
                before,
                name,
                category,
            } => self.save_tag(before.as_ref(), name, category.as_ref()),
            Request::DeleteTag(tag) => self.delete_tag(tag),
            Request::Category { before, name } => self.save_category(before.as_ref(), name),
            Request::DeleteCategory { before, tags } => self.delete_category(before, tags),
            Request::Playlists => Ok(Reply::Playlists(self.list("api/playlists")?)),
            Request::Playlist {
                before,
                name,
                parent,
                folder,
            } => self.save_playlist(before.as_ref(), name, parent.as_ref(), *folder),
            Request::DeletePlaylist { id, before } => self.delete_playlist(*id, before),
            Request::Members { before, ids, added } => self.save_members(before, ids, added),
        }
    }

    fn tag_catalog(&self) -> Result<Catalog, String> {
        let catalog = Catalog {
            tags: self.list("api/tags")?,
            categories: self.list("api/tag-categories")?,
        };
        catalog.validate()?;
        Ok(catalog)
    }

    fn edit_track_snapshot(&self, expected: &Track) -> Result<TrackSnapshot, String> {
        let track: Track = self.get(&format!("api/tracks/{}", expected.id))?;
        editing::same_track(expected, &track)?;
        let catalog = self.tag_catalog()?;
        let tags: Vec<Tag> = self.list(&format!("api/tracks/{}/tags", expected.id))?;
        if tags.iter().any(|tag| !catalog.tags.contains(tag)) {
            return Err("Tags changed while loading; reload before editing".into());
        }
        Ok(TrackSnapshot {
            track,
            catalog,
            tags: tags.into_iter().map(|t| t.id).collect(),
        })
    }

    // Never replay mutations. A transport error may follow an applied write.
    fn edit_write(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, String> {
        self.json(method, path, body, 15).map_err(|error| format!("Save was not confirmed; some changes may have applied. Reload and review before trying again. {error}"))
    }

    fn save_track(
        &self,
        before: &TrackSnapshot,
        rating: u8,
        color: u8,
        desired: &BTreeSet<u32>,
    ) -> Result<Reply, String> {
        if rating > 5 || color > 8 {
            return Err("Invalid rating or color".into());
        }
        let current = self.edit_track_snapshot(&before.track)?;
        let rating_changed = rating != before.track.rating;
        let color_changed = color != before.track.color_id;
        if (rating_changed && current.track.rating != before.track.rating)
            || (color_changed && current.track.color_id != before.track.color_id)
        {
            return Err("Rating or color changed elsewhere. Reload before saving.".into());
        }
        let changed: BTreeSet<_> = before.tags.symmetric_difference(desired).copied().collect();
        for id in &changed {
            let expected = before
                .catalog
                .tags
                .iter()
                .find(|t| t.id == *id)
                .ok_or("Unknown tag; reload")?;
            if !current.catalog.tags.contains(expected)
                || current.tags.contains(id) != before.tags.contains(id)
            {
                return Err("A tag being edited changed elsewhere. Reload before saving.".into());
            }
        }
        let base = format!("api/tracks/{}", before.track.id);
        if rating_changed {
            self.edit_write(
                Method::PUT,
                &format!("{base}/rating"),
                Some(json!({"rating":rating})),
            )?;
        }
        if color_changed {
            self.edit_write(
                Method::PUT,
                &format!("{base}/color"),
                Some(json!({"color_id":color})),
            )?;
        }
        if !changed.is_empty() {
            let add: Vec<_> = desired.difference(&before.tags).copied().collect();
            let remove: Vec<_> = before.tags.difference(desired).copied().collect();
            self.edit_write(
                Method::POST,
                "api/tracks/tags",
                Some(json!({"track_ids":[before.track.id],"add":add,"remove":remove})),
            )?;
        }
        let after = self
            .edit_track_snapshot(&before.track)
            .map_err(readback_error)?;
        if (rating_changed && after.track.rating != rating)
            || (color_changed && after.track.color_id != color)
            || changed
                .iter()
                .any(|id| after.tags.contains(id) != desired.contains(id))
        {
            return Err("Readback differs from your changes. Reload and review.".into());
        }
        Ok(Reply::Track(Box::new(after)))
    }

    fn catalog_reply(&self, catalog: Catalog) -> Result<Reply, String> {
        // Definition changes may not bump the collection revision on older daemons.
        Ok(Reply::Catalog(
            catalog,
            Some(self.list("api/tracks").map_err(readback_error)?),
        ))
    }

    fn save_tag(
        &self,
        before: Option<&Tag>,
        value: &str,
        category: Option<&Category>,
    ) -> Result<Reply, String> {
        let name = editing::name(value)?;
        let catalog = self.tag_catalog()?;
        if category.is_some_and(|c| !catalog.categories.contains(c)) {
            return Err("Category changed; reload tags".into());
        }
        let category_id = category.map_or(0, |c| c.id);
        if before.is_some_and(|t| !catalog.tags.contains(t)) {
            return Err("Tag changed; reload before editing".into());
        }
        if catalog.tags.iter().any(|t| {
            t.name == name && t.category_id == category_id && before.is_none_or(|b| b.id != t.id)
        }) {
            return Err("That tag already exists in this category".into());
        }
        let body = Some(json!({"name":name,"category_id":category_id}));
        let id = if let Some(tag) = before {
            self.edit_write(Method::PUT, &format!("api/tags/{}", tag.id), body)?;
            tag.id
        } else {
            let tag: Tag =
                serde_json::from_value(self.edit_write(Method::POST, "api/tags", body)?)
                    .map_err(|e| readback_error(e.to_string()))?;
            if tag.id == 0 || catalog.tags.iter().any(|t| t.id == tag.id) {
                return Err("Created tag identity was not confirmed; reload".into());
            }
            tag.id
        };
        let after = self.tag_catalog().map_err(readback_error)?;
        if !after
            .tags
            .iter()
            .any(|t| t.id == id && t.name == name && t.category_id == category_id)
        {
            return Err("Tag edit was not confirmed; reload".into());
        }
        self.catalog_reply(after)
    }

    fn delete_tag(&self, before: &Tag) -> Result<Reply, String> {
        if !self.tag_catalog()?.tags.contains(before) {
            return Err("Tag changed; reload before deleting".into());
        }
        self.edit_write(Method::DELETE, &format!("api/tags/{}", before.id), None)?;
        let after = self.tag_catalog().map_err(readback_error)?;
        if after.tags.iter().any(|t| t.id == before.id) {
            return Err("Tag deletion was not confirmed; reload".into());
        }
        self.catalog_reply(after)
    }

    fn save_category(&self, before: Option<&Category>, value: &str) -> Result<Reply, String> {
        let name = editing::name(value)?;
        let catalog = self.tag_catalog()?;
        if before.is_some_and(|c| !catalog.categories.contains(c)) {
            return Err("Category changed; reload before editing".into());
        }
        if catalog
            .categories
            .iter()
            .any(|c| c.name == name && before.is_none_or(|b| b.id != c.id))
        {
            return Err("A category already has that name".into());
        }
        let body = Some(json!({"name":name}));
        let id = if let Some(category) = before {
            self.edit_write(
                Method::PUT,
                &format!("api/tag-categories/{}", category.id),
                body,
            )?;
            category.id
        } else {
            let category: Category = serde_json::from_value(self.edit_write(
                Method::POST,
                "api/tag-categories",
                body,
            )?)
            .map_err(|e| readback_error(e.to_string()))?;
            if category.id == 0 || catalog.categories.iter().any(|c| c.id == category.id) {
                return Err("Created category identity was not confirmed; reload".into());
            }
            category.id
        };
        let after = self.tag_catalog().map_err(readback_error)?;
        if !after
            .categories
            .iter()
            .any(|c| c.id == id && c.name == name)
        {
            return Err("Category edit was not confirmed; reload".into());
        }
        self.catalog_reply(after)
    }

    fn delete_category(&self, before: &Category, tags: &[Tag]) -> Result<Reply, String> {
        let catalog = self.tag_catalog()?;
        let members: Vec<_> = catalog
            .tags
            .iter()
            .filter(|t| t.category_id == before.id)
            .cloned()
            .collect();
        if !catalog.categories.contains(before) || members != tags {
            return Err("Category contents changed; reload before deleting".into());
        }
        self.edit_write(
            Method::DELETE,
            &format!("api/tag-categories/{}", before.id),
            None,
        )?;
        let after = self.tag_catalog().map_err(readback_error)?;
        if after.categories.iter().any(|c| c.id == before.id)
            || members.iter().any(|old| {
                !after
                    .tags
                    .iter()
                    .any(|t| t.id == old.id && t.name == old.name && t.category_id == 0)
            })
        {
            return Err("Category deletion was not confirmed; reload".into());
        }
        self.catalog_reply(after)
    }

    fn save_playlist(
        &self,
        before: Option<&Playlist>,
        value: &str,
        parent: Option<&Playlist>,
        folder: bool,
    ) -> Result<Reply, String> {
        let name = editing::name(value)?;
        let current: Vec<Playlist> = self.list("api/playlists")?;
        if parent.is_some_and(|p| !p.is_folder || !editing::editable(p) || !current.contains(p)) {
            return Err("Destination folder changed; reload playlists".into());
        }
        let parent_id = parent.map_or(0, |p| p.id);
        let id = if let Some(before) = before {
            if !editing::editable(before) || !current.contains(before) {
                return Err("Playlist changed or is read-only; reload before saving".into());
            }
            if editing::descendants(&current, before.id).contains(&parent_id) {
                return Err("A folder cannot contain itself".into());
            }
            self.edit_write(
                Method::PUT,
                &format!("api/playlists/{}", before.id),
                Some(json!({"name":name,"parent_id":parent_id})),
            )?;
            before.id
        } else {
            let created: Playlist = serde_json::from_value(self.edit_write(
                Method::POST,
                "api/playlists",
                Some(json!({"name":name,"parent_id":parent_id,"is_folder":folder})),
            )?)
            .map_err(|e| readback_error(e.to_string()))?;
            if created.id == 0 || current.iter().any(|p| p.id == created.id) {
                return Err("Created playlist identity was not confirmed; reload".into());
            }
            created.id
        };
        let after: Vec<Playlist> = self.list("api/playlists").map_err(readback_error)?;
        if !after.iter().any(|p| {
            p.id == id && p.name == name && p.parent_id == parent_id && p.is_folder == folder
        }) {
            return Err("Playlist edit was not confirmed; reload".into());
        }
        Ok(Reply::Playlists(after))
    }

    fn delete_playlist(&self, id: u32, before: &[Playlist]) -> Result<Reply, String> {
        let current: Vec<Playlist> = self.list("api/playlists")?;
        let ids = editing::descendants(before, id);
        let current_ids = editing::descendants(&current, id);
        if !before.iter().any(|p| p.id == id)
            || ids != current_ids
            || before
                .iter()
                .filter(|p| ids.contains(&p.id))
                .any(|p| !editing::editable(p) || !current.contains(p))
        {
            return Err("Playlist or folder contents changed; reload before deleting".into());
        }
        self.edit_write(Method::DELETE, &format!("api/playlists/{id}"), None)?;
        let after: Vec<Playlist> = self.list("api/playlists").map_err(readback_error)?;
        if after.iter().any(|p| ids.contains(&p.id)) {
            return Err("Playlist deletion was not confirmed; reload".into());
        }
        Ok(Reply::Playlists(after))
    }

    fn save_members(
        &self,
        before: &Playlist,
        ids: &[u32],
        added: &[Track],
    ) -> Result<Reply, String> {
        let current: Vec<Playlist> = self.list("api/playlists")?;
        if !editing::editable(before)
            || before.is_folder
            || before.is_smart
            || !current.contains(before)
        {
            return Err("Playlist changed or membership is read-only; reload".into());
        }
        if ids.contains(&0)
            || ids.iter().any(|id| {
                ids.iter().filter(|n| *n == id).count()
                    > before.track_ids.iter().filter(|n| *n == id).count().max(1)
            })
        {
            return Err("Invalid or duplicate playlist entries".into());
        }
        for id in ids.iter().filter(|id| !before.track_ids.contains(id)) {
            let expected = added
                .iter()
                .find(|t| t.id == *id)
                .ok_or("Select the added track again")?;
            editing::same_track(expected, &self.get(&format!("api/tracks/{id}"))?)?;
        }
        let appending = ids.len() > before.track_ids.len() && ids.starts_with(&before.track_ids);
        if appending {
            // The server appends to its current list, preserving concurrent additions.
            self.edit_write(Method::POST, "api/tracks/playlists", Some(json!({"track_ids":&ids[before.track_ids.len()..],"add":[before.id],"remove":[]})))?;
        } else {
            self.edit_write(
                Method::POST,
                &format!("api/playlists/{}/tracks", before.id),
                Some(json!({"track_ids":ids})),
            )?;
        }
        let after: Vec<Playlist> = self.list("api/playlists").map_err(readback_error)?;
        if !after.iter().any(|p| {
            p.id == before.id
                && !p.is_folder
                && !p.is_smart
                && if appending {
                    ids[before.track_ids.len()..]
                        .iter()
                        .all(|id| p.track_ids.contains(id))
                } else {
                    p.track_ids == ids
                }
        }) {
            return Err("Playlist membership was not confirmed; reload".into());
        }
        Ok(Reply::Playlists(after))
    }
}

fn readback_error(error: String) -> String {
    format!("Could not verify the saved values. Reload and review. {error}")
}
