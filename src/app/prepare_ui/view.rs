// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(in crate::app) fn prepare_view(&mut self, ui: &mut Ui) {
        let playlist = self.prepare_playlist().cloned();
        let on_deck = self.on_deck_snapshot();
        ui.horizontal(|ui| {
            let title_width = (ui.available_width() - 264.0).max(80.0);
            ui.add_sized(
                vec2(title_width, 48.0),
                Label::new(
                    RichText::new(playlist.as_ref().map_or("Prepare".into(), |p| {
                        format!("{} · {} tracks", p.name, p.track_ids.len())
                    }))
                    .size(23.0),
                )
                .truncate(),
            );
            let choose = ui
                .add_enabled_ui(self.prepare.pending.is_none() && self.library_ready, |ui| {
                    theme::button(ui, "Choose playlist…", vec2(160.0, 48.0), false)
                })
                .inner;
            self.hit("Prepare choose", choose.rect);
            if choose.clicked() {
                self.choose_prepare(None);
            }
            let reload = ui
                .add_enabled_ui(self.prepare.pending.is_none() && self.library_ready, |ui| {
                    theme::button(ui, "Reload", vec2(88.0, 48.0), false)
                })
                .inner;
            self.hit("Prepare reload", reload.rect);
            if reload.clicked() {
                self.prepare_submit(Request::Playlists, Action::Read);
            }
        });
        if self.prepare.pending.is_some() {
            ui.label(RichText::new("Updating Prepare…").color(MUTED));
        }
        if !self.prepare.error.is_empty() {
            ui.add(Label::new(RichText::new(&self.prepare.error).color(ORANGE).size(13.0)).wrap());
        }
        let Some(playlist) = playlist else {
            ui.add_space(12.0);
            let message = if self.prepare.binding.is_some() && !self.options.demo {
                "The saved playlist is unavailable or the library session changed. Choose your Prepare playlist again."
            } else {
                "Choose a regular playlist or create a Prepare playlist to keep tracks ready for later."
            };
            ui.label(message);
            ui.label(
                RichText::new("The playlist is shared with your other Vynull clients.")
                    .color(MUTED),
            );
            return;
        };
        Panel::bottom("prepare-actions")
            .exact_size(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG))
            .show(ui, |ui| self.prepare_actions(ui, &playlist));
        if playlist.track_ids.is_empty() {
            ui.add_space(16.0);
            ui.heading("Ready for your next tracks");
            ui.label("Tap Prepare on a track in Collection, Inspect or Match.");
            ui.label(
                RichText::new("Loading a track keeps it in the list until you remove it.")
                    .color(MUTED),
            );
            return;
        }
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let id = self.prepare_scroll_id();
            let rows = track_scroll::show_rows(
                ui,
                id,
                (ui.available_height() - 8.0).max(20.0),
                58.0,
                playlist.track_ids.len(),
                |ui, range| {
                    for index in range {
                        let id = playlist.track_ids[index];
                        let track = self.all_tracks.iter().find(|t| t.id == id).cloned();
                        let selected = self.prepare.selected.as_ref().is_some_and(|s| {
                            s.index == index
                                && s.id == id
                                && s.path.as_ref() == track.as_ref().map(|t| &t.file_path)
                        });
                        let (rect, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
                        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        painter.rect_filled(
                            rect,
                            0,
                            if selected {
                                SELECTED
                            } else if index % 2 == 0 {
                                PANEL
                            } else {
                                BG
                            },
                        );
                        theme::text(
                            &painter,
                            Rect::from_min_size(rect.min + vec2(4.0, 0.0), vec2(30.0, 58.0)),
                            (index + 1).to_string(),
                            12.0,
                            MUTED,
                        );
                        let cover =
                            Rect::from_min_size(rect.min + vec2(38.0, 5.0), vec2(48.0, 48.0));
                        let texture = track
                            .as_ref()
                            .filter(|_| ui.is_rect_visible(rect))
                            .and_then(|track| {
                                self.artwork.as_mut().and_then(|cache| cache.texture(track))
                            });
                        if let Some(texture) = texture {
                            Image::new((texture, cover.size())).paint_at(ui, cover);
                        } else {
                            theme::sleeve(&painter, cover, id);
                        }
                        let text_left = cover.right() + 12.0;
                        let text_right = rect.right() - 188.0;
                        let title = track.as_ref().map_or_else(
                            || format!("Unavailable track #{id}"),
                            |t| t.title.clone(),
                        );
                        let subtitle =
                            track
                                .as_ref()
                                .map_or("Removed from the library".into(), |t| {
                                    if t.file_missing {
                                        format!("{} · File missing", t.artist)
                                    } else {
                                        t.artist.clone()
                                    }
                                });
                        theme::text(
                            &painter,
                            Rect::from_min_max(
                                pos2(text_left, rect.top() + 5.0),
                                pos2(text_right, rect.top() + 30.0),
                            ),
                            title,
                            17.0,
                            TEXT,
                        );
                        let mut subtitle_rect = Rect::from_min_max(
                            pos2(text_left, rect.top() + 30.0),
                            pos2(text_right, rect.bottom() - 5.0),
                        );
                        if let Some(track) = &track {
                            subtitle_rect = self.paint_on_deck(ui, subtitle_rect, track, &on_deck);
                        }
                        theme::text(&painter, subtitle_rect, subtitle, 13.0, MUTED);
                        if let Some(track) = &track {
                            for (offset, width, value) in [
                                (
                                    180.0,
                                    70.0,
                                    if track.bpm.is_finite() && track.bpm > 0.0 {
                                        format!("{:.1}", track.bpm)
                                    } else {
                                        "—".into()
                                    },
                                ),
                                (102.0, 46.0, track.key.clone()),
                                (52.0, 48.0, time(f64::from(track.duration))),
                            ] {
                                theme::text(
                                    &painter,
                                    Rect::from_min_size(
                                        pos2(rect.right() - offset, rect.top()),
                                        vec2(width, 58.0),
                                    ),
                                    value,
                                    14.0,
                                    TEXT,
                                );
                            }
                        }
                        self.hit(&format!("Prepare track {index}"), rect);
                        if response.clicked() {
                            self.prepare.selected = Some(Selection {
                                index,
                                id,
                                path: track.map(|t| t.file_path),
                            });
                        }
                    }
                },
            );
            self.hit("Prepare tracks", rows.inner_rect);
        });
    }

    fn prepare_actions(&mut self, ui: &mut Ui, playlist: &Playlist) {
        let selection = self.prepare.selected.clone().filter(|s| {
            playlist.track_ids.get(s.index) == Some(&s.id)
                && self
                    .all_tracks
                    .iter()
                    .find(|t| t.id == s.id)
                    .map(|t| &t.file_path)
                    == s.path.as_ref()
        });
        let track = self.prepare_selected_track().cloned();
        ui.horizontal(|ui| {
            for (label, delta) in [("Up", -1_isize), ("Down", 1)] {
                let target = selection
                    .as_ref()
                    .and_then(|s| s.index.checked_add_signed(delta))
                    .filter(|i| *i < playlist.track_ids.len());
                let response = ui
                    .add_enabled_ui(self.prepare_writable() && target.is_some(), |ui| {
                        theme::button(ui, label, vec2(80.0, 48.0), false)
                    })
                    .inner;
                self.hit(&format!("Prepare {label}"), response.rect);
                if response.clicked()
                    && let (Some(mut selected), Some(target)) = (selection.clone(), target)
                {
                    let mut ids = playlist.track_ids.clone();
                    ids.swap(selected.index, target);
                    selected.index = target;
                    self.prepare_submit(
                        Request::Members {
                            before: playlist.clone(),
                            ids,
                            added: vec![],
                        },
                        Action::Members(Some(selected)),
                    );
                }
            }
            let remove = ui
                .add_enabled_ui(self.prepare_writable() && selection.is_some(), |ui| {
                    theme::button(ui, "Remove", vec2(104.0, 48.0), false)
                })
                .inner
                .on_hover_text("Remove from this playlist; keep the track in your library");
            self.hit("Prepare remove", remove.rect);
            if remove.clicked()
                && let Some(selected) = &selection
            {
                let mut ids = playlist.track_ids.clone();
                ids.remove(selected.index);
                self.prepare_submit(
                    Request::Members {
                        before: playlist.clone(),
                        ids,
                        added: vec![],
                    },
                    Action::Members(None),
                );
            }
            let inspect = ui
                .add_enabled_ui(track.is_some(), |ui| {
                    theme::button(ui, "Inspect", vec2(96.0, 48.0), false)
                })
                .inner;
            self.hit("Prepare inspect", inspect.rect);
            if inspect.clicked()
                && let Some(track) = &track
            {
                self.inspect(track.clone());
            }
            let load = ui
                .add_enabled_ui(
                    track.as_ref().is_some_and(|t| !t.file_missing)
                        && self.fresh()
                        && !self.load_busy
                        && self.pending_load.is_none(),
                    |ui| theme::primary(ui, "Load to…", vec2(116.0, 48.0)),
                )
                .inner;
            self.hit("Prepare load", load.rect);
            if load.clicked() {
                self.load_dialog = true;
            }
        });
    }

    pub(in crate::app) fn prepare_picker(&mut self, ctx: &Context) {
        let mut close = false;
        let modal = Modal::new(Id::unique("prepare-picker")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(640.0));
            ui.heading("Choose your Prepare playlist");
            ui.label(
                RichText::new(if self.prepare.adding.is_some() {
                    "Choose a playlist to add this track. Its other tracks will stay in the list."
                } else {
                    "Use a regular playlist shared with your other Vynull clients."
                })
                .size(14.0)
                .color(MUTED),
            );
            ui.horizontal(|ui| {
                let cancel = ui
                    .add_enabled_ui(self.prepare.pending.is_none(), |ui| {
                        theme::button(ui, "Cancel", vec2(100.0, 48.0), false)
                    })
                    .inner;
                self.hit("Prepare cancel", cancel.rect);
                close = cancel.clicked();
                let exists = self
                    .playlists
                    .iter()
                    .any(|p| p.parent_id == 0 && p.name == "Prepare");
                let create = ui
                    .add_enabled_ui(
                        self.prepare.ready && self.prepare_writable() && !exists,
                        |ui| theme::button(ui, "Create Prepare", vec2(176.0, 48.0), false),
                    )
                    .inner;
                self.hit("Prepare create", create.rect);
                if create.clicked() {
                    self.prepare_submit(
                        Request::Playlist {
                            before: None,
                            name: "Prepare".into(),
                            parent: None,
                            folder: false,
                        },
                        Action::Create(self.playlists.iter().map(|p| p.id).collect()),
                    );
                }
                let reload = ui
                    .add_enabled_ui(self.prepare.pending.is_none(), |ui| {
                        theme::button(ui, "Reload", vec2(88.0, 48.0), false)
                    })
                    .inner;
                self.hit("Prepare picker reload", reload.rect);
                if reload.clicked() {
                    self.prepare_submit(Request::Playlists, Action::Read);
                }
            });
            if self.prepare.pending.is_some() {
                ui.label("Loading and verifying playlist…");
            }
            if !self.prepare.error.is_empty() {
                ui.add(
                    Label::new(RichText::new(&self.prepare.error).size(13.0).color(ORANGE)).wrap(),
                );
            }
            let playlists: Vec<_> = self
                .playlists
                .iter()
                .filter(|p| prepare::eligible(p))
                .cloned()
                .collect();
            ui.add_enabled_ui(self.prepare.ready && self.prepare_writable(), |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                track_scroll::show_rows(
                    ui,
                    Id::unique("prepare-playlists"),
                    (ctx.content_rect().height() - 260.0).clamp(100.0, 320.0),
                    54.0,
                    playlists.len(),
                    |ui, range| {
                        for index in range {
                            let playlist = &playlists[index];
                            let parent = self
                                .playlists
                                .iter()
                                .find(|p| p.id == playlist.parent_id)
                                .map(|p| p.name.as_str());
                            let label = format!(
                                "{}{} · {} tracks",
                                parent.map_or(String::new(), |name| format!("{name} / ")),
                                playlist.name,
                                playlist.track_ids.len()
                            );
                            let (rect, response) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 54.0),
                                Sense::click(),
                            );
                            ui.painter()
                                .rect_filled(rect.shrink2(vec2(0.0, 3.0)), 6, CARD);
                            theme::text(
                                ui.painter(),
                                rect.shrink2(vec2(12.0, 0.0)),
                                label,
                                16.0,
                                TEXT,
                            );
                            self.hit(&format!("Prepare playlist {}", playlist.id), rect);
                            if response.clicked() {
                                self.bind_prepare(playlist.id);
                            }
                        }
                    },
                );
            });
        });
        if self.prepare.pending.is_none() && (close || modal.should_close()) {
            self.prepare.picker = false;
            self.prepare.adding = None;
        }
    }
}
