// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::matching::live::Reference;

impl App {
    pub(super) fn library_match_player(&self) -> Option<&Player> {
        self.library_match
            .player(&self.status, self.fresh() && self.linked)
    }

    fn library_match_description(&self) -> String {
        if let Some(player) = self.library_match_player() {
            return format!(
                "Player {} · {} BPM · {}",
                player.device_number,
                crate::matching::tempo(player).map_or("Unknown".into(), |bpm| format!("{bpm:.1}")),
                if player.key.trim().is_empty() {
                    "Unknown key"
                } else {
                    &player.key
                },
            );
        }
        match self.library_match {
            Reference::Off => "Match highlighting is off".into(),
            _ if !self.fresh() || !self.linked => "Waiting for live player status".into(),
            Reference::Master => "Waiting for a master deck with a loaded track".into(),
            Reference::Player(number) => format!("Waiting for Player {number} with a loaded track"),
        }
    }

    pub(super) fn library_match_button(&mut self, ui: &mut Ui) {
        let player = self.library_match_player();
        let label = match self.library_match {
            Reference::Off => "Match: Off".into(),
            Reference::Master => player.map_or("Match: Auto".into(), |p| {
                format!("Match: M{}", p.device_number)
            }),
            Reference::Player(number) => format!("Match: P{number}"),
        };
        let button = ui
            .add_sized(
                vec2(112.0, 48.0),
                Button::new(RichText::new(label).size(14.0).color(if player.is_some() {
                    GREEN
                } else {
                    MUTED
                }))
                .fill(CARD)
                .corner_radius(8),
            )
            .on_hover_text(self.library_match_description());
        self.hit("Library match", button.rect);
        if button.clicked() {
            self.keyboard = None;
            self.library_match_picker = true;
        }
    }

    pub(in crate::app) fn library_match_modal(&mut self, ctx: &Context) {
        let mut close = false;
        let modal = Modal::new(Id::unique("library-match")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(600.0));
            ui.heading("Match highlighting");
            ui.horizontal(|ui| {
                for (reference, label) in [
                    (Reference::Master, "Auto · Master"),
                    (Reference::Off, "Off"),
                ] {
                    let button = theme::button(
                        ui,
                        label,
                        vec2(164.0, 48.0),
                        self.library_match == reference,
                    );
                    self.hit(&format!("Highlight {label}"), button.rect);
                    if button.clicked() {
                        self.library_match = reference;
                    }
                }
            });
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 24.0) / 4.0;
                for number in 1..=4 {
                    let reference = Reference::Player(number);
                    let enabled = reference
                        .player(&self.status, self.fresh() && self.linked)
                        .is_some();
                    let button = ui
                        .add_enabled_ui(enabled, |ui| {
                            theme::button(
                                ui,
                                &format!("Player {number}"),
                                vec2(width, 48.0),
                                self.library_match == reference,
                            )
                        })
                        .inner;
                    self.hit(&format!("Highlight player {number}"), button.rect);
                    if button.clicked() {
                        self.library_match = reference;
                    }
                }
            });
            ui.add(Label::new(self.library_match_description()).truncate());
            let title = self
                .library_match_player()
                .map_or("", |p| p.track_title.as_str());
            ui.add(Label::new(RichText::new(title).color(MUTED)).truncate());
            ui.horizontal(|ui| {
                ui.label("BPM range");
                for tolerance in [3.0, 6.0, 10.0] {
                    let label = format!("±{tolerance:.0}%");
                    let button = theme::button(
                        ui,
                        &label,
                        vec2(70.0, 48.0),
                        self.matching.tolerance == tolerance,
                    );
                    self.hit(&format!("Highlight {label}"), button.rect);
                    if button.clicked() {
                        self.matching.tolerance = tolerance;
                        self.matching.dirty = true;
                    }
                }
                let half = theme::button(
                    ui,
                    "½ / ×2 tempo",
                    vec2(156.0, 48.0),
                    self.matching.half_double,
                );
                self.hit("Highlight half double", half.rect);
                if half.clicked() {
                    self.matching.half_double = !self.matching.half_double;
                    self.matching.dirty = true;
                }
            });
            ui.label("Green cells mark compatible keys or BPMs within the selected range.");
            ui.horizontal(|ui| {
                self.key_matching_button(ui, "Key rules…", 144.0);
                ui.add(
                    Label::new(
                        RichText::new(
                            "Same key always matches. Choose which neighbors also match.",
                        )
                        .size(13.0)
                        .color(MUTED),
                    )
                    .wrap(),
                );
            });
            let done = theme::primary(ui, "Done", vec2(ui.available_width(), 48.0));
            self.hit("Highlight done", done.rect);
            close |= done.clicked();
        });
        self.hit("Highlight dialog", modal.response.rect);
        if close || modal.should_close() {
            self.library_match_picker = false;
        }
    }
}
