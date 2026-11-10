// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(super) fn save_players(&mut self) {
        let players = settings::Players {
            pair: self.deck_pair,
            compare: self.deck_compare,
            waveform_span: self.deck_span,
        };
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_players(path, players)
        {
            self.notice = format!("Player view changed, but could not save the settings: {error}");
        }
    }

    pub(in crate::app) fn player_pair_modal(&mut self, ctx: &Context) {
        let Some(mut pair) = self.deck_picker else {
            return;
        };
        let mut numbers: Vec<_> = (1..=4)
            .chain(self.deck_pair)
            .chain(pair)
            .chain(
                self.status
                    .peers
                    .iter()
                    .filter(|p| p.device_type == "cdj")
                    .map(|p| p.device_number),
            )
            .chain(self.status.players.iter().map(|p| p.device_number))
            .filter(|n| *n > 0)
            .collect();
        numbers.sort_unstable();
        numbers.dedup();
        let mut close = false;
        Modal::new(Id::unique("player-pair")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(504.0));
            ui.heading("Choose players");
            ui.label(
                RichText::new("Set the primary pair for Cards and Waveforms.")
                    .size(14.0)
                    .color(MUTED),
            );
            ScrollArea::vertical()
                .id_salt("player-pair-options")
                .max_height((ctx.content_rect().height() - 220.0).max(100.0))
                .scroll_source(scroll_area::ScrollSource::ALL)
                .show(ui, |ui| {
                    for (index, label) in ["Top / left player", "Bottom / right player"]
                        .into_iter()
                        .enumerate()
                    {
                        ui.label(label);
                        ui.horizontal_wrapped(|ui| {
                            for &number in &numbers {
                                let button = ui
                                    .add_enabled_ui(number != pair[1 - index], |ui| {
                                        theme::button(
                                            ui,
                                            &number.to_string(),
                                            vec2(64.0, 48.0),
                                            pair[index] == number,
                                        )
                                    })
                                    .inner;
                                self.hit(&format!("Pair {index} player {number}"), button.rect);
                                if button.clicked() {
                                    pair[index] = number;
                                }
                            }
                        });
                    }
                });
            ui.label(
                RichText::new("Disconnected players stay available.")
                    .size(12.0)
                    .color(MUTED),
            );
            ui.horizontal(|ui| {
                let swap = theme::button(ui, "Swap", vec2(96.0, 48.0), false);
                self.hit("Pair swap", swap.rect);
                if swap.clicked() {
                    pair.swap(0, 1);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let apply = theme::primary(ui, "Use pair", vec2(120.0, 48.0));
                    self.hit("Pair apply", apply.rect);
                    if apply.clicked() {
                        self.deck_pair = pair;
                        self.save_players();
                        close = true;
                    }
                    let cancel = theme::button(ui, "Cancel", vec2(96.0, 48.0), false);
                    self.hit("Pair cancel", cancel.rect);
                    if cancel.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.deck_picker = (!close).then_some(pair);
    }
}
