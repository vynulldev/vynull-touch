// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(super) fn open_matches(&mut self) {
        if self.fresh()
            && self.matching.seed.is_none()
            && let Some(player) = self
                .status
                .players
                .iter()
                .find(|p| p.is_master)
                .or(self.status.players.first())
                .cloned()
        {
            self.matching.select_player(player);
        }
        self.page = Page::Match;
        self.preview.stop();
    }

    pub(super) fn match_view(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let back = theme::button(ui, "‹ Players", vec2(120.0, 48.0), false);
            self.hit("Match back", back.rect);
            if back.clicked() {
                self.page = Page::Players;
            }
            let players = self.status.players.clone();
            for player in players {
                let active = self
                    .matching
                    .seed
                    .as_ref()
                    .is_some_and(|p| p.device_number == player.device_number);
                let response = ui
                    .add_enabled_ui(self.fresh(), |ui| {
                        theme::button(
                            ui,
                            &format!("Player {}", player.device_number),
                            vec2(104.0, 48.0),
                            active,
                        )
                    })
                    .inner;
                self.hit(
                    &format!("Match player {}", player.device_number),
                    response.rect,
                );
                if response.clicked() {
                    self.matching.select_player(player);
                }
            }
        });
        let Some(seed) = self.matching.seed.clone() else {
            ui.label("Choose a connected player to find matching tracks.");
            return;
        };
        let tempo = crate::matching::tempo(&seed);
        ui.add(
            Label::new(format!(
                "{} · {} BPM · {}",
                seed.track_title,
                tempo.map_or("Unknown".into(), |b| format!("{b:.1}")),
                seed.key
            ))
            .truncate(),
        );
        ui.label(RichText::new("Matches use this captured tempo and the reported key. Tap the player again to refresh.").size(12.0).color(MUTED));
        if !self.fresh() {
            ui.colored_label(
                ORANGE,
                "Player status unavailable; showing the last captured match settings.",
            );
        } else if !self
            .status
            .players
            .iter()
            .any(|p| p.same_loaded_track(&seed))
        {
            ui.colored_label(
                ORANGE,
                "Player track changed. Tap its button to update matches.",
            );
        }
        ui.horizontal(|ui| {
            for tolerance in [3.0, 6.0, 10.0] {
                let label = format!("±{tolerance:.0}%");
                let response = theme::button(
                    ui,
                    &label,
                    vec2(70.0, 48.0),
                    self.matching.tolerance == tolerance,
                );
                self.hit(&format!("Match {label}"), response.rect);
                if response.clicked() {
                    self.matching.tolerance = tolerance;
                    self.matching.dirty = true;
                }
            }
            let keys = theme::button(ui, "Compatible keys", vec2(180.0, 48.0), self.matching.keys);
            self.hit("Match keys", keys.rect);
            if keys.clicked() {
                self.matching.keys = !self.matching.keys;
                self.matching.dirty = true;
            }
            let half = theme::button(
                ui,
                "½ / ×2 tempo",
                vec2(156.0, 48.0),
                self.matching.half_double,
            );
            self.hit("Match half double", half.rect);
            if half.clicked() {
                self.matching.half_double = !self.matching.half_double;
                self.matching.dirty = true;
            }
        });
        self.matching.update(&self.all_tracks);
        ui.horizontal(|ui| {
            ui.label(format!("{} matches", self.matching.results.len()));
            let selected = self.selected_track().cloned();
            ui.add_enabled_ui(selected.is_some(), |ui| {
                let inspect = theme::button(ui, "Inspect", vec2(96.0, 48.0), false);
                self.hit("Match inspect", inspect.rect);
                if inspect.clicked()
                    && let Some(track) = &selected
                {
                    self.inspect(track.clone());
                }
                let add = theme::button(ui, "Add to playlist", vec2(164.0, 48.0), false);
                self.hit("Match add to playlist", add.rect);
                if add.clicked()
                    && let Some(track) = &selected
                {
                    self.open_playlist_manager(Some(track.clone()));
                }
            });
            ui.add_enabled_ui(
                selected.is_some()
                    && self.fresh()
                    && !self.load_busy
                    && self.pending_load.is_none(),
                |ui| {
                    let load = theme::button(ui, "Load to…", vec2(116.0, 48.0), false);
                    self.hit("Match load", load.rect);
                    if load.clicked() {
                        self.load_dialog = true;
                    }
                },
            );
        });
        if tempo.is_none() {
            ui.label("This player has no usable tempo. Choose another player.");
        } else if self.matching.results.is_empty() {
            ui.label("No matches. Widen the tempo range or turn off compatible keys.");
        }
        ScrollArea::vertical()
            .scroll_source(scroll_area::ScrollSource::ALL)
            .show(ui, |ui| {
                for item in &self.matching.results {
                    let Some(track) = self
                        .all_tracks
                        .iter()
                        .find(|t| t.id == item.id && t.file_path == item.path)
                    else {
                        continue;
                    };
                    let label = format!(
                        "{} · {}\n{} · {:.1} BPM · {}",
                        track.title, track.artist, item.reason, track.bpm, track.key
                    );
                    let response = theme::button(
                        ui,
                        &label,
                        vec2(ui.available_width(), 58.0),
                        self.matching.selected == Some(track.id),
                    );
                    #[cfg(test)]
                    self.hits
                        .push((format!("Match track {}", track.id), response.rect));
                    if response.clicked() {
                        self.matching.selected = Some(track.id);
                    }
                }
            });
    }
}
