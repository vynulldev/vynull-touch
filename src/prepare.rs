// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{editing, model::Playlist};
use serde::{Deserialize, Serialize};

/// A playlist ID is only meaningful within its server and loaded library.
/// The playlist itself lives on Vynull; this is just the local shortcut.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Binding {
    pub server: String,
    pub session: String,
    pub playlist: u32,
}

pub fn eligible(playlist: &Playlist) -> bool {
    playlist.id != 0 && editing::editable(playlist) && !playlist.is_folder && !playlist.is_smart
}

impl Binding {
    pub fn matches(&self, server: &str, session: Option<&str>) -> bool {
        self.server.trim_end_matches('/') == server.trim_end_matches('/')
            && session == Some(self.session.as_str())
            && !self.session.is_empty()
    }
}
