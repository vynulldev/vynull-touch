// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::HashMap;

pub struct Demo {
    catalog: Catalog,
    assignments: HashMap<u32, BTreeSet<u32>>,
    next_tag: u32,
    next_category: u32,
    next_playlist: u32,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            catalog: Catalog {
                tags: vec![Tag {
                    id: 1,
                    name: "Demo".into(),
                    category_id: 1,
                    color: 0,
                }],
                categories: vec![Category {
                    id: 1,
                    name: "Collection".into(),
                }],
            },
            assignments: HashMap::new(),
            next_tag: 2,
            next_category: 2,
            next_playlist: 100,
        }
    }
}

impl Demo {
    fn snapshot(&mut self, expected: &Track, tracks: &[Track]) -> Result<TrackSnapshot, String> {
        let track = tracks
            .iter()
            .find(|t| t.id == expected.id)
            .ok_or("Track removed")?
            .clone();
        same_track(expected, &track)?;
        let tags = self
            .assignments
            .entry(track.id)
            .or_insert_with(|| {
                self.catalog
                    .tags
                    .iter()
                    .filter(|t| track.tags.contains(&t.name))
                    .map(|t| t.id)
                    .collect()
            })
            .clone();
        Ok(TrackSnapshot {
            track,
            catalog: self.catalog.clone(),
            tags,
        })
    }

    pub fn run(
        &mut self,
        request: &Request,
        tracks: &[Track],
        playlists: &[Playlist],
    ) -> Result<Reply, String> {
        match request {
            Request::Track(track) => {
                return Ok(Reply::Track(Box::new(self.snapshot(track, tracks)?)));
            }
            Request::SaveTrack {
                before,
                rating,
                color,
                tags,
            } => {
                let mut after = self.snapshot(&before.track, tracks)?;
                after.track.rating = *rating;
                after.track.color_id = *color;
                after.tags = tags.clone();
                after.track.tags = self
                    .catalog
                    .tags
                    .iter()
                    .filter(|t| tags.contains(&t.id))
                    .map(|t| t.name.clone())
                    .collect();
                self.assignments.insert(after.track.id, tags.clone());
                return Ok(Reply::Track(Box::new(after)));
            }
            Request::Catalog => return Ok(Reply::Catalog(self.catalog.clone(), None)),
            Request::Tag {
                before,
                name: value,
                category,
            } => {
                let name = name(value)?.to_owned();
                let category_id = category.as_ref().map_or(0, |c| c.id);
                if self.catalog.tags.iter().any(|t| {
                    t.name == name
                        && t.category_id == category_id
                        && before.as_ref().is_none_or(|b| b.id != t.id)
                }) {
                    return Err("That tag already exists".into());
                }
                if let Some(before) = before {
                    let tag = self
                        .catalog
                        .tags
                        .iter_mut()
                        .find(|t| t.id == before.id)
                        .ok_or("Tag removed")?;
                    tag.name = name;
                    tag.category_id = category_id;
                } else {
                    self.catalog.tags.push(Tag {
                        id: self.next_tag,
                        name,
                        category_id,
                        color: 0,
                    });
                    self.next_tag += 1;
                }
            }
            Request::DeleteTag(tag) => {
                self.catalog.tags.retain(|t| t.id != tag.id);
                for ids in self.assignments.values_mut() {
                    ids.remove(&tag.id);
                }
            }
            Request::Category {
                before,
                name: value,
            } => {
                let name = name(value)?.to_owned();
                if self
                    .catalog
                    .categories
                    .iter()
                    .any(|c| c.name == name && before.as_ref().is_none_or(|b| b.id != c.id))
                {
                    return Err("A category already has that name".into());
                }
                if let Some(before) = before {
                    self.catalog
                        .categories
                        .iter_mut()
                        .find(|c| c.id == before.id)
                        .ok_or("Category removed")?
                        .name = name;
                } else {
                    self.catalog.categories.push(Category {
                        id: self.next_category,
                        name,
                    });
                    self.next_category += 1;
                }
            }
            Request::DeleteCategory { before, .. } => {
                self.catalog.categories.retain(|c| c.id != before.id);
                for tag in &mut self.catalog.tags {
                    if tag.category_id == before.id {
                        tag.category_id = 0;
                    }
                }
            }
            Request::Playlists => return Ok(Reply::Playlists(playlists.to_vec())),
            Request::Playlist {
                before,
                name: value,
                parent,
                folder,
            } => {
                let mut after = playlists.to_vec();
                let name = name(value)?.to_owned();
                let parent_id = parent.as_ref().map_or(0, |p| p.id);
                if let Some(before) = before {
                    if descendants(playlists, before.id).contains(&parent_id) {
                        return Err("A folder cannot contain itself".into());
                    }
                    let p = after
                        .iter_mut()
                        .find(|p| p.id == before.id)
                        .ok_or("Playlist removed")?;
                    p.name = name;
                    p.parent_id = parent_id;
                } else {
                    after.push(Playlist {
                        id: self.next_playlist,
                        name,
                        parent_id,
                        is_folder: *folder,
                        ..Default::default()
                    });
                    self.next_playlist += 1;
                }
                return Ok(Reply::Playlists(after));
            }
            Request::DeletePlaylist { id, .. } => {
                let ids = descendants(playlists, *id);
                return Ok(Reply::Playlists(
                    playlists
                        .iter()
                        .filter(|p| !ids.contains(&p.id))
                        .cloned()
                        .collect(),
                ));
            }
            Request::Members { before, ids, .. } => {
                let mut after = playlists.to_vec();
                after
                    .iter_mut()
                    .find(|p| p.id == before.id)
                    .ok_or("Playlist removed")?
                    .track_ids = ids.clone();
                return Ok(Reply::Playlists(after));
            }
        }
        // Keep ID-based assignments when a library-wide tag is renamed/deleted.
        let mut after = tracks.to_vec();
        for track in &mut after {
            let ids = self.assignments.entry(track.id).or_insert_with(|| {
                if track.tags.iter().any(|t| t == "Demo") {
                    BTreeSet::from([1])
                } else {
                    BTreeSet::new()
                }
            });
            track.tags = self
                .catalog
                .tags
                .iter()
                .filter(|t| ids.contains(&t.id))
                .map(|t| t.name.clone())
                .collect();
        }
        Ok(Reply::Catalog(self.catalog.clone(), Some(after)))
    }
}
