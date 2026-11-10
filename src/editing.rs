// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::{Playlist, Track};
use serde::Deserialize;
use std::collections::BTreeSet;
mod demo;
pub use demo::Demo;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Tag {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub category_id: u32,
    #[serde(default)]
    pub color: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Category {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub tags: Vec<Tag>,
    pub categories: Vec<Category>,
}

impl Catalog {
    pub fn category(&self, id: u32) -> &str {
        if id == 0 {
            return "Uncategorized";
        }
        self.categories
            .iter()
            .find(|c| c.id == id)
            .map_or("Unknown category", |c| c.name.as_str())
    }

    pub fn validate(&self) -> Result<(), String> {
        for ids in [
            self.tags.iter().map(|t| t.id).collect::<Vec<_>>(),
            self.categories.iter().map(|c| c.id).collect(),
        ] {
            if ids.contains(&0) || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
                return Err("Invalid tag catalog; reload before editing".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct TrackSnapshot {
    pub track: Track,
    pub catalog: Catalog,
    pub tags: BTreeSet<u32>,
}

#[derive(Clone, Debug)]
pub enum Request {
    Track(Track),
    SaveTrack {
        before: Box<TrackSnapshot>,
        rating: u8,
        color: u8,
        tags: BTreeSet<u32>,
    },
    Catalog,
    Tag {
        before: Option<Tag>,
        name: String,
        category: Option<Category>,
    },
    DeleteTag(Tag),
    Category {
        before: Option<Category>,
        name: String,
    },
    DeleteCategory {
        before: Category,
        tags: Vec<Tag>,
    },
    Playlists,
    Playlist {
        before: Option<Playlist>,
        name: String,
        parent: Option<Playlist>,
        folder: bool,
    },
    DeletePlaylist {
        id: u32,
        before: Vec<Playlist>,
    },
    Members {
        before: Playlist,
        ids: Vec<u32>,
        added: Vec<Track>,
    },
}

impl Request {
    pub fn writes(&self) -> bool {
        !matches!(self, Self::Track(_) | Self::Catalog | Self::Playlists)
    }
}

pub enum Reply {
    Track(Box<TrackSnapshot>),
    Catalog(Catalog, Option<Vec<Track>>),
    Playlists(Vec<Playlist>),
}

pub fn name(value: &str) -> Result<&str, String> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 128 || value.chars().any(char::is_control) {
        Err("Enter a name of 1–128 characters".into())
    } else {
        Ok(value)
    }
}

pub fn same_track(expected: &Track, current: &Track) -> Result<(), String> {
    if expected.id == 0 || expected.id != current.id || expected.file_path != current.file_path {
        Err("Track source changed; close and select it again".into())
    } else {
        Ok(())
    }
}

pub fn editable(playlist: &Playlist) -> bool {
    !playlist.read_only && playlist.id & (1 << 30) == 0
}

pub fn descendants(playlists: &[Playlist], id: u32) -> BTreeSet<u32> {
    let mut ids = BTreeSet::from([id]);
    loop {
        let size = ids.len();
        for p in playlists {
            if ids.contains(&p.parent_id) {
                ids.insert(p.id);
            }
        }
        if ids.len() == size {
            return ids;
        }
    }
}
