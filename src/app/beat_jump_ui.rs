// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

impl App {
    fn beat_jump_target(&self, direction: i32, duration: f64) -> Option<f64> {
        let target = self.detail.as_ref()?.beat_grid.jump(
            self.inspect_seek_position(self.position, duration),
            direction * i32::from(self.beat_jump_beats),
            duration,
        )?;
        Some(self.inspect_seek_position(target, duration))
    }

    fn beat_jump_button(&mut self, ui: &mut Ui, direction: i32, duration: f64) {
        let target = self.beat_jump_target(direction, duration);
        let enabled = target
            .is_some_and(|target| (target - self.position) * f64::from(direction) > 0.000_001);
        let name = if direction < 0 { "back" } else { "forward" };
        let response = ui
            .add_enabled_ui(enabled, |ui| {
                theme::button(
                    ui,
                    if direction < 0 { "‹" } else { "›" },
                    vec2(48.0, 48.0),
                    false,
                )
            })
            .inner
            .on_hover_text(if target.is_some() {
                format!(
                    "Jump {name} {} beats in local preview",
                    self.beat_jump_beats
                )
            } else {
                "Beat jump needs at least two saved beats".into()
            });
        self.hit(&format!("Beat jump {name}"), response.rect);
        if response.clicked()
            && let Some(target) = target
        {
            self.position = target;
            self.wave_drag_start = None;
            self.loop_error.clear();
            self.preview.seek(target, false);
        }
    }

    pub(super) fn beat_jump_controls(&mut self, ui: &mut Ui, duration: f64) {
        self.beat_jump_button(ui, -1, duration);
        let size = theme::button(
            ui,
            &format!("Jump {}", self.beat_jump_beats),
            vec2(104.0, 48.0),
            false,
        )
        .on_hover_text("Choose the number of beats to jump");
        self.hit("Beat jump size", size.rect);
        Popup::menu(&size).show(|ui| {
            ui.label("Beat jump size");
            for row in settings::BEAT_JUMP_SIZES.chunks(3) {
                ui.horizontal(|ui| {
                    for &beats in row {
                        let label =
                            format!("{beats} {}", if beats == 1 { "beat" } else { "beats" });
                        let choice = theme::button(
                            ui,
                            &label,
                            vec2(100.0, 48.0),
                            self.beat_jump_beats == beats,
                        );
                        self.hit(&format!("Beat jump size {beats}"), choice.rect);
                        if choice.clicked() {
                            self.beat_jump_beats = beats;
                            if let Some(path) = &self.settings_path
                                && let Err(error) = settings::save_beat_jump(path, beats)
                            {
                                self.notice = format!("Could not save beat jump size: {error}");
                            }
                            ui.close();
                        }
                    }
                });
            }
        });
        self.beat_jump_button(ui, 1, duration);
    }
}
