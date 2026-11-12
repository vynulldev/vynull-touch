// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    editing::{Reply, Request},
    prepare::{self, Binding},
};

#[cfg(test)]
mod tests;
mod view;

#[derive(Clone)]
struct Selection {
    index: usize,
    id: u32,
    path: Option<String>,
}

enum Action {
    Read,
    Create(Vec<u32>),
    Add,
    Remove,
    Members(Option<Selection>),
}

pub(super) struct Pending {
    pub token: u64,
    action: Action,
}

pub(super) struct State {
    binding: Option<Binding>,
    demo_playlist: Option<u32>,
    selected: Option<Selection>,
    pub pending: Option<Pending>,
    pub picker: bool,
    ready: bool,
    adding: Option<Track>,
    error: String,
    reload: bool,
}

impl State {
    pub(super) fn new(binding: Option<Binding>) -> Self {
        Self {
            binding,
            demo_playlist: None,
            selected: None,
            pending: None,
            picker: false,
            ready: false,
            adding: None,
            error: String::new(),
            reload: false,
        }
    }

    pub(super) fn reset(&mut self) {
        *self = Self::new(self.binding.take());
    }
}

impl App {
    fn prepare_playlist(&self) -> Option<&Playlist> {
        let id = if self.options.demo {
            self.prepare.demo_playlist?
        } else {
            let binding = self.prepare.binding.as_ref()?;
            if !binding.matches(&self.options.server, self.library_session.as_deref()) {
                return None;
            }
            binding.playlist
        };
        self.playlists
            .iter()
            .find(|p| p.id == id && prepare::eligible(p))
    }

    pub(super) fn prepare_selected_track(&self) -> Option<&Track> {
        let selection = self.prepare.selected.as_ref()?;
        let playlist = self.prepare_playlist()?;
        if playlist.track_ids.get(selection.index) != Some(&selection.id) {
            return None;
        }
        self.all_tracks
            .iter()
            .find(|t| t.id == selection.id && Some(&t.file_path) == selection.path.as_ref())
    }

    pub(super) fn prepared(&self, track: &Track) -> bool {
        self.prepare_playlist()
            .is_some_and(|p| p.track_ids.contains(&track.id))
            && self
                .all_tracks
                .iter()
                .any(|t| t.id == track.id && t.file_path == track.file_path)
    }

    fn prepare_writable(&self) -> bool {
        self.library_ready
            && self.fresh()
            && (self.options.demo || self.library_session.is_some())
            && self.prepare.pending.is_none()
            && !self.prepare.reload
            && self.editor.is_none()
    }

    pub(super) fn prepare_scroll_id(&self) -> Id {
        Id::unique((
            "prepare-tracks",
            self.library_session.as_deref(),
            self.prepare_playlist().map(|p| p.id),
        ))
    }

    fn prepare_error(&mut self, error: String) {
        self.notice = error.clone();
        self.prepare.error = error;
        self.prepare.reload = true;
        // A lost reply can follow an applied write. Never replay a queued add.
        self.prepare.adding = None;
    }

    fn prepare_submit(&mut self, request: Request, action: Action) {
        if self.prepare.pending.is_some() {
            return;
        }
        if request.writes() && !self.prepare_writable() {
            return;
        }
        self.edit_serial = self.edit_serial.wrapping_add(1);
        self.prepare.pending = Some(Pending {
            token: self.edit_serial,
            action,
        });
        self.prepare.error.clear();
        // Discard any refresh started before this foreground snapshot or write.
        self.refresh.generation = self.refresh.generation.wrapping_add(1);
        self.refresh.pending = None;
        self.refresh.force = true;
        self.refresh.next = Instant::now();
        if self.options.demo {
            let result = self
                .demo_edits
                .run(&request, &self.all_tracks, &self.playlists);
            self.prepare_response(result);
        } else if !self.send(Command::Edit(self.edit_serial, Box::new(request))) {
            self.prepare_response(Err(
                "Request was not sent. Open Prepare and reload before trying again.".into(),
            ));
        }
    }

    pub(super) fn prepare_response(&mut self, result: Result<Reply, String>) {
        let Some(pending) = self.prepare.pending.take() else {
            return;
        };
        let playlists = match result {
            Ok(Reply::Playlists(playlists)) => playlists,
            Ok(_) => {
                self.prepare_error("Unexpected playlist response. Reload Prepare.".into());
                return;
            }
            Err(error) => {
                self.prepare_error(error);
                return;
            }
        };
        let page = self.page;
        self.apply_edited_playlists(&playlists);
        // An unrelated browsing playlist may have disappeared. Clear that
        // source without pulling the user out of Prepare, Match or Inspect.
        self.page = page;
        self.prepare.ready = true;
        self.prepare.reload = false;
        self.prepare.error.clear();
        match pending.action {
            Action::Read => {}
            Action::Create(before) => {
                let created: Vec<_> = playlists
                    .iter()
                    .filter(|p| {
                        !before.contains(&p.id)
                            && p.name == "Prepare"
                            && p.parent_id == 0
                            && prepare::eligible(p)
                    })
                    .collect();
                if let [playlist] = created.as_slice() {
                    self.bind_prepare(playlist.id);
                } else {
                    self.prepare_error("Playlist creation could not be identified. Reload and choose the playlist from the list.".into());
                }
            }
            Action::Add => self.notice = "Added to Prepare · saved and verified".into(),
            Action::Remove => {
                self.prepare.selected = None;
                self.notice = "Removed from Prepare · saved and verified".into();
            }
            Action::Members(selection) => {
                self.prepare.selected = selection;
                self.notice = "Prepare playlist saved and verified".into();
            }
        }
    }

    fn choose_prepare(&mut self, adding: Option<Track>) {
        if self.prepare.pending.is_some() {
            return;
        }
        self.keyboard = None;
        self.prepare.picker = true;
        self.prepare.ready = false;
        self.prepare.adding = adding;
        self.prepare_submit(Request::Playlists, Action::Read);
    }

    fn bind_prepare(&mut self, id: u32) {
        if !self
            .playlists
            .iter()
            .any(|p| p.id == id && prepare::eligible(p))
        {
            return;
        }
        if self.options.demo {
            self.prepare.demo_playlist = Some(id);
        } else {
            let Some(session) = self.library_session.clone() else {
                return;
            };
            let binding = Binding {
                server: self.options.server.clone(),
                session,
                playlist: id,
            };
            if let Some(path) = &self.settings_path
                && let Err(error) = settings::save_prepare_playlist(path, &binding)
            {
                self.prepare_error(format!("Could not remember the Prepare playlist: {error}"));
                return;
            }
            self.prepare.binding = Some(binding);
        }
        self.prepare.selected = None;
        self.prepare.picker = false;
        self.notice = "Prepare playlist selected".into();
        if let Some(track) = self.prepare.adding.take() {
            self.add_to_prepare(track);
        }
    }

    fn add_to_prepare(&mut self, track: Track) {
        if !self.prepare_writable() || self.prepared(&track) {
            return;
        }
        if !self
            .all_tracks
            .iter()
            .any(|t| t.id == track.id && t.file_path == track.file_path)
        {
            self.prepare_error("Track changed or was removed. Select it again.".into());
            return;
        }
        let Some(before) = self.prepare_playlist().cloned() else {
            self.choose_prepare(Some(track));
            return;
        };
        let mut ids = before.track_ids.clone();
        ids.push(track.id);
        self.prepare_submit(
            Request::Members {
                before,
                ids,
                added: vec![track],
            },
            Action::Add,
        );
    }

    pub(super) fn prepare_button(&mut self, ui: &mut Ui, track: &Track) {
        let prepared = self.prepared(track);
        let response = ui
            .add_enabled_ui(self.prepare_writable() && !prepared, |ui| {
                theme::button(
                    ui,
                    if prepared { "Prepared" } else { "Prepare" },
                    vec2(112.0, 48.0),
                    prepared,
                )
            })
            .inner;
        self.hit("Prepare add", response.rect);
        if response.clicked() {
            self.add_to_prepare(track.clone());
        }
    }

    fn remove_from_prepare(&mut self, track: &Track) {
        if !self.prepare_writable() || !self.prepared(track) {
            return;
        }
        let Some(before) = self.prepare_playlist().cloned() else {
            return;
        };
        let ids = before
            .track_ids
            .iter()
            .copied()
            .filter(|id| *id != track.id)
            .collect();
        self.prepare_submit(
            Request::Members {
                before,
                ids,
                added: vec![],
            },
            Action::Remove,
        );
    }

    pub(super) fn prepare_icon(&mut self, ui: &mut Ui, rect: Rect, track: &Track) {
        let prepared = self.prepared(track);
        let enabled = self.prepare_writable() && !self.grid_pending();
        let response = ui.place(
            rect,
            Button::new("")
                .fill(if prepared { SELECTED } else { CARD })
                .sense(if enabled {
                    Sense::click()
                } else {
                    Sense::hover()
                })
                .corner_radius(8),
        );
        theme::icon(
            ui.painter(),
            rect.center(),
            5,
            if prepared {
                GREEN
            } else if enabled {
                TEXT
            } else {
                MUTED
            },
        );
        self.hit("Prepare add", response.rect);
        if response.clicked() {
            if prepared {
                self.remove_from_prepare(track);
            } else {
                self.add_to_prepare(track.clone());
            }
        }
        response.on_hover_text(if prepared {
            "Remove from Prepare"
        } else {
            "Add to Prepare"
        });
    }
}
