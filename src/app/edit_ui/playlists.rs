// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(super) fn apply_edited_playlists(&mut self, playlists: &[Playlist]) {
        self.playlists = playlists.to_vec();
        if self.folder != 0 && !playlists.iter().any(|p| p.id == self.folder && p.is_folder) {
            self.folder = 0;
        }
        if let Some(id) = self.source {
            if let Some(p) = playlists.iter().find(|p| p.id == id && !p.is_folder) {
                self.source_name = p.name.clone();
                if !p.is_smart {
                    self.tracks = p
                        .track_ids
                        .iter()
                        .filter_map(|id| self.all_tracks.iter().find(|t| t.id == *id).cloned())
                        .collect();
                    self.reindex();
                }
            } else {
                self.source(None);
            }
        }
    }

    pub(super) fn playlist_edit_controls(
        &mut self,
        ui: &mut Ui,
        manager: &mut Manager,
        request: &mut Option<Request>,
    ) {
        let mut next = None;
        match &mut manager.view {
            View::Playlists => {
                ui.horizontal(|ui| {
                    if self.edit_button(ui, "New playlist", 160.0, false) {
                        next = Some(View::PlaylistForm {
                            before: None,
                            name: String::new(),
                            parent: self.folder,
                            folder: false,
                        });
                    }
                    if manager.adding.is_none() && self.edit_button(ui, "New folder", 150.0, false)
                    {
                        next = Some(View::PlaylistForm {
                            before: None,
                            name: String::new(),
                            parent: self.folder,
                            folder: true,
                        });
                    }
                    if self.edit_button(ui, "Search…", 120.0, false) {
                        manager.typing = true;
                    }
                });
                if let Some(track) = &manager.adding {
                    ui.add(Label::new(format!("Add: {}", track.title)).truncate());
                } else {
                    ui.label("Choose a playlist or folder to rename, move, or edit its contents.");
                }
                for p in &manager.playlists {
                    if manager.adding.is_some()
                        && (p.is_folder || p.is_smart || !editing::editable(p))
                    {
                        continue;
                    }
                    let path = playlist_path(&manager.playlists, p);
                    if !manager.query.is_empty()
                        && !path.to_lowercase().contains(&manager.query.to_lowercase())
                    {
                        continue;
                    }
                    let already = manager
                        .adding
                        .as_ref()
                        .is_some_and(|t| p.track_ids.contains(&t.id));
                    let label = format!(
                        "{}{}{}",
                        path,
                        if already { " · Already added" } else { "" },
                        if !editing::editable(p) {
                            " · Read-only"
                        } else if p.is_folder {
                            " · Folder"
                        } else if p.is_smart {
                            " · Smart"
                        } else {
                            ""
                        }
                    );
                    ui.add_enabled_ui(editing::editable(p) && !already, |ui| {
                        if self.edit_button(ui, &label, ui.available_width(), false) {
                            if let Some(track) = &manager.adding {
                                let mut ids = p.track_ids.clone();
                                ids.push(track.id);
                                *request = Some(Request::Members {
                                    before: p.clone(),
                                    ids,
                                    added: vec![track.clone()],
                                });
                                manager.after = Some(View::Playlists);
                            } else {
                                next = Some(View::PlaylistForm {
                                    before: Some(p.clone()),
                                    name: p.name.clone(),
                                    parent: p.parent_id,
                                    folder: p.is_folder,
                                });
                            }
                        }
                    });
                }
            }
            View::PlaylistForm {
                before,
                name,
                parent,
                folder,
            } => {
                self.edit_name(ui, name, &mut manager.typing);
                ui.label("Folder");
                let exclude = before.as_ref().map_or_else(BTreeSet::new, |p| {
                    editing::descendants(&manager.playlists, p.id)
                });
                ScrollArea::vertical()
                    .id_salt("playlist-parent-choice")
                    .max_height(110.0)
                    .scroll_source(scroll_area::ScrollSource::ALL)
                    .show(ui, |ui| {
                        if self.edit_button(ui, "Top level", ui.available_width(), *parent == 0) {
                            *parent = 0;
                        }
                        for p in manager.playlists.iter().filter(|p| {
                            p.is_folder && editing::editable(p) && !exclude.contains(&p.id)
                        }) {
                            if self.edit_button(
                                ui,
                                &playlist_path(&manager.playlists, p),
                                ui.available_width(),
                                *parent == p.id,
                            ) {
                                *parent = p.id;
                            }
                        }
                    });
                ui.horizontal_wrapped(|ui| {
                    if self.edit_button(ui, "Cancel", 100.0, false) {
                        next = Some(View::Playlists);
                    }
                    ui.add_enabled_ui(editing::name(name).is_ok(), |ui| {
                        if self.edit_button(ui, "Save playlist", 160.0, false) {
                            *request = Some(Request::Playlist {
                                before: before.clone(),
                                name: name.clone(),
                                parent: manager.playlists.iter().find(|p| p.id == *parent).cloned(),
                                folder: *folder,
                            });
                            manager.after = Some(View::Playlists);
                        }
                    });
                    if let Some(p) = before {
                        if !p.is_folder
                            && !p.is_smart
                            && self.edit_button(ui, "Edit tracks", 140.0, false)
                        {
                            next = Some(View::Members {
                                before: p.clone(),
                                ids: p.track_ids.clone(),
                                selected: None,
                            });
                        }
                        if self.edit_button(ui, "Delete…", 110.0, false) {
                            let count = editing::descendants(&manager.playlists, p.id)
                                .len()
                                .saturating_sub(1);
                            manager.confirm = Some((
                                format!(
                                    "Delete ‘{}’{}? Music files stay in the collection.",
                                    p.name,
                                    if p.is_folder {
                                        format!(" and its {count} nested playlists/folders")
                                    } else {
                                        String::new()
                                    }
                                ),
                                Request::DeletePlaylist {
                                    id: p.id,
                                    before: manager.playlists.clone(),
                                },
                            ));
                            manager.after = Some(View::Playlists);
                        }
                    }
                });
                if before.as_ref().is_some_and(|p| p.is_smart) {
                    ui.label("Smart playlist membership is determined by its rules.");
                }
            }
            View::Members {
                before,
                ids,
                selected,
            } => {
                ui.add(Label::new(&before.name).truncate());
                ui.horizontal(|ui| {
                    if self.edit_button(ui, "Cancel", 100.0, false) {
                        next = Some(View::Playlists);
                    }
                    ui.add_enabled_ui(*ids != before.track_ids, |ui| {
                        if self.edit_button(ui, "Save order", 140.0, false) {
                            *request = Some(Request::Members {
                                before: before.clone(),
                                ids: ids.clone(),
                                added: vec![],
                            });
                            manager.after = Some(View::Playlists);
                        }
                    });
                    ui.add_enabled_ui(selected.is_some_and(|i| i > 0 && i < ids.len()), |ui| {
                        if self.edit_button(ui, "↑", 64.0, false) {
                            let i = selected.unwrap();
                            ids.swap(i, i - 1);
                            *selected = Some(i - 1);
                        }
                    });
                    ui.add_enabled_ui(selected.is_some_and(|i| i + 1 < ids.len()), |ui| {
                        if self.edit_button(ui, "↓", 64.0, false) {
                            let i = selected.unwrap();
                            ids.swap(i, i + 1);
                            *selected = Some(i + 1);
                        }
                    });
                    ui.add_enabled_ui(selected.is_some_and(|i| i < ids.len()), |ui| {
                        if self.edit_button(ui, "Remove", 110.0, false) {
                            ids.remove(selected.unwrap());
                            *selected = None;
                        }
                    });
                });
                ui.label("Select a track, then move it or remove it. Save applies the new order.");
                for (i, id) in ids.iter().enumerate() {
                    let title = self.all_tracks.iter().find(|t| t.id == *id).map_or_else(
                        || format!("Unavailable track #{id}"),
                        |t| format!("{} · {}", t.title, t.artist),
                    );
                    if self.edit_button(
                        ui,
                        &format!("{}. {title}", i + 1),
                        ui.available_width(),
                        *selected == Some(i),
                    ) {
                        *selected = Some(i);
                    }
                }
                if ids.is_empty() {
                    ui.label("This playlist is empty. Add a track from Inspect or the collection selection menu.");
                }
            }
            _ => (),
        }
        if let Some(view) = next {
            manager.view = view;
            manager.query.clear();
        }
    }
}

fn playlist_path(playlists: &[Playlist], playlist: &Playlist) -> String {
    let mut names = vec![playlist.name.as_str()];
    let mut visited = BTreeSet::from([playlist.id]);
    let mut parent = playlist.parent_id;
    while parent != 0 && visited.insert(parent) {
        let Some(p) = playlists.iter().find(|p| p.id == parent) else {
            break;
        };
        names.push(&p.name);
        parent = p.parent_id;
    }
    names.reverse();
    names.join(" / ")
}
