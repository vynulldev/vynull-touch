// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::loops::Region;

#[cfg(test)]
mod tests;

impl App {
    fn loop_length(&self, duration: f64) -> Option<usize> {
        self.detail
            .as_ref()?
            .beat_grid
            .loop_beats(self.preview.request.loop_region?, duration)
    }
    fn set_loop_length(&mut self, beats: usize, duration: f64) {
        let Some(detail) = &self.detail else {
            return;
        };
        let current = self.preview.request.loop_region;
        let start = current.map_or(self.position, |r| r.start);
        match detail.beat_grid.loop_region(start, beats, duration) {
            Ok(region) => {
                self.loop_error.clear();
                if current.is_some() {
                    self.preview.change_loop(Some(region), self.position);
                    self.position = region.position(self.position);
                } else {
                    self.position = region.start;
                    self.preview.start_loop(region);
                }
            }
            Err(error) => self.loop_error = error,
        }
    }
    fn save_loop_editor(&mut self, duration: f64) {
        if !self.can_edit_cues() {
            return;
        }
        let (Some(region), Some(detail)) = (self.preview.request.loop_region, &self.detail) else {
            return;
        };
        let slot = (1..=8)
            .find(|n| !detail.cues.iter().any(|c| c.number == *n))
            .unwrap_or(1);
        let mut editor = crate::cues::Editor::new(
            &detail.cues,
            slot,
            self.position,
            duration,
            &detail.beat_grid,
        );
        editor.loop_template = Some(region);
        editor = editor.select(
            &detail.cues,
            slot,
            self.position,
            duration,
            &detail.beat_grid,
        );
        self.preview.pause();
        self.cue_editor = Some(editor);
    }
    pub(super) fn loop_toolbar(&mut self, ui: &mut Ui, duration: f64) {
        let region = self.preview.request.loop_region;
        let beats = self.loop_length(duration);
        ui.horizontal(|ui| {
            let (label, _) = ui.allocate_exact_size(vec2(116.0, 48.0), Sense::hover());
            theme::text(
                ui.painter(),
                Rect::from_min_size(label.min, vec2(116.0, 24.0)),
                time(self.position),
                14.0,
                TEXT,
            );
            theme::text(
                ui.painter(),
                Rect::from_min_size(label.min + vec2(0.0, 24.0), vec2(116.0, 24.0)),
                if let Some(beats) = beats {
                    format!("{beats} beat loop")
                } else if region.is_some() {
                    "Saved loop".into()
                } else if self.detail.as_ref().is_some_and(|d| d.beat_grid.is_empty()) {
                    "No beat grid".into()
                } else {
                    "Loop off".into()
                },
                12.0,
                MUTED,
            );
            let grid = self
                .detail
                .as_ref()
                .is_some_and(|d| !d.beat_grid.is_empty());
            for count in [4, 8, 16, 32] {
                let response = ui
                    .add_enabled_ui(grid && duration > 0.0, |ui| {
                        theme::button(
                            ui,
                            &count.to_string(),
                            vec2(48.0, 48.0),
                            beats == Some(count),
                        )
                    })
                    .inner;
                self.hit(&format!("Loop {count}"), response.rect);
                if response.clicked() {
                    self.set_loop_length(count, duration);
                }
            }
            for (label, target) in [
                (
                    "½",
                    beats
                        .filter(|b| *b > 1 && b.is_multiple_of(2))
                        .map(|b| b / 2),
                ),
                ("×2", beats.filter(|b| *b <= 32).map(|b| b * 2)),
            ] {
                let response = ui
                    .add_enabled_ui(target.is_some(), |ui| {
                        theme::button(ui, label, vec2(48.0, 48.0), false)
                    })
                    .inner;
                self.hit(&format!("Loop {label}"), response.rect);
                if response.clicked()
                    && let Some(beats) = target
                {
                    self.set_loop_length(beats, duration);
                }
            }
            let exit = ui
                .add_enabled_ui(region.is_some(), |ui| {
                    theme::button(ui, "Exit", vec2(64.0, 48.0), false)
                })
                .inner;
            self.hit("Loop exit", exit.rect);
            if exit.clicked() {
                self.preview.change_loop(None, self.position);
                self.loop_error.clear();
            }
            let save = ui
                .add_enabled_ui(region.is_some() && self.can_edit_cues(), |ui| {
                    theme::button(ui, "Save…", vec2(90.0, 48.0), false)
                })
                .inner;
            self.hit("Loop save", save.rect);
            if save.clicked() {
                self.save_loop_editor(duration);
            }
        });
    }
    pub(super) fn paint_loop(
        &self,
        painter: &Painter,
        rect: Rect,
        view: std::ops::Range<f64>,
        region: Region,
    ) {
        if region.end < view.start || region.start > view.end || view.end <= view.start {
            return;
        }
        let x = |seconds| {
            rect.left() + ((seconds - view.start) / (view.end - view.start)) as f32 * rect.width()
        };
        let band = Rect::from_x_y_ranges(
            x(region.start.max(view.start))..=x(region.end.min(view.end)),
            rect.y_range(),
        );
        painter.rect_filled(band, 2, GREEN.gamma_multiply(0.23));
        for (seconds, label, align) in [
            (region.start, "IN", Align2::LEFT_TOP),
            (region.end, "OUT", Align2::RIGHT_TOP),
        ] {
            if view.contains(&seconds) || seconds == view.end {
                painter.vline(x(seconds), rect.y_range(), Stroke::new(2.0, GREEN));
                if band.width() >= 48.0 {
                    painter.text(
                        pos2(x(seconds), rect.top()),
                        align,
                        label,
                        FontId::proportional(9.0),
                        GREEN,
                    );
                }
            }
        }
    }
}
