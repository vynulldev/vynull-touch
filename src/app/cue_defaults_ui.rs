// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::cues;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Picker {
    slot: usize,
    error: String,
}

impl App {
    fn set_cue_default(&mut self, picker: &mut Picker, color: u32) {
        let mut colors = self.hot_cue_colors;
        colors[picker.slot] = color;
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_hot_cue_colors(path, colors)
        {
            picker.error = format!("Could not save cue colors: {error}");
            return;
        }
        self.hot_cue_colors = colors;
        picker.error.clear();
    }

    pub(super) fn cue_defaults_modal(&mut self, ctx: &Context) {
        let Some(mut picker) = self.cue_defaults.take() else {
            return;
        };
        let mut close = false;
        let modal = Modal::new(Id::unique("cue-defaults")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(600.0));
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.heading("Default cue colors");
            ui.label(
                RichText::new("New hot cues and saved loops use these colors on this screen.")
                    .size(14.0)
                    .color(MUTED),
            );
            ui.label(
                RichText::new("Existing cues keep their colors.")
                    .size(14.0)
                    .color(MUTED),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for index in 0..8 {
                    let label = cues::slot(index as u16 + 1);
                    let response = ui.add_sized(
                        vec2(56.0, 48.0),
                        Button::new(RichText::new(&label).size(22.0).color(Color32::BLACK))
                            .fill(cues::color(self.hot_cue_colors[index]))
                            .stroke(Stroke::new(
                                if picker.slot == index { 3.0 } else { 0.0 },
                                TEXT,
                            )),
                    );
                    self.hit(&format!("Default cue slot {label}"), response.rect);
                    if response.clicked() {
                        picker.slot = index;
                    }
                }
            });
            let selected = self.hot_cue_colors[picker.slot];
            let name = if selected == 0 {
                "Default"
            } else {
                cues::PALETTE
                    .iter()
                    .find(|(id, _)| *id == selected)
                    .map_or("Custom color", |(_, name)| *name)
            };
            ui.label(format!(
                "Cue {} · {name}",
                cues::slot(picker.slot as u16 + 1)
            ));
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    for row in cues::PALETTE.chunks(8) {
                        ui.horizontal(|ui| {
                            for &(id, name) in row {
                                let response = ui.add_sized(
                                    vec2(48.0, 48.0),
                                    Button::new("").fill(cues::color(id)).stroke(Stroke::new(
                                        if selected == id { 2.0 } else { 0.0 },
                                        TEXT,
                                    )),
                                );
                                self.hit(&format!("Default cue color {id}"), response.rect);
                                if response.clicked() {
                                    self.set_cue_default(&mut picker, id);
                                }
                                response.on_hover_text(name);
                            }
                        });
                    }
                });
                let reset = theme::button(ui, "Default", vec2(112.0, 48.0), selected == 0);
                self.hit("Default cue color 0", reset.rect);
                if reset.clicked() {
                    self.set_cue_default(&mut picker, 0);
                }
            });
            if !picker.error.is_empty() {
                ui.add(Label::new(RichText::new(&picker.error).size(13.0).color(ORANGE)).wrap());
            }
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Changes save automatically.")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let done = theme::primary(ui, "Done", vec2(112.0, 48.0));
                    self.hit("Cue defaults done", done.rect);
                    close = done.clicked();
                });
            });
        });
        if !close && !modal.should_close() {
            self.cue_defaults = Some(picker);
        }
    }
}
