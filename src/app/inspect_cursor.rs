// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

impl App {
    pub(super) fn inspect_seek_position(&self, seconds: f64, duration: f64) -> f64 {
        let seconds = seconds.clamp(0.0, duration.max(0.0));
        if self.inspect_quantize && !self.grid_editing() {
            self.detail
                .as_ref()
                .and_then(|detail| detail.beat_grid.nearest(seconds, duration))
                .unwrap_or(seconds)
        } else {
            seconds
        }
    }

    pub(super) fn quantize_button(&mut self, ui: &mut Ui, duration: f64) {
        let available = self
            .detail
            .as_ref()
            .is_some_and(|detail| detail.beat_grid.nearest(self.position, duration).is_some());
        let button = ui
            .add_enabled_ui(available, |ui| {
                theme::button(
                    ui,
                    "Quantize",
                    vec2(96.0, 48.0),
                    available && self.inspect_quantize,
                )
            })
            .inner
            .on_hover_text(if available {
                "Snap waveform taps, drags and beat jumps to saved beats"
            } else {
                "No saved beat inside this track; waveform positioning is free"
            });
        self.hit("Inspect quantize", button.rect);
        if button.clicked() {
            self.inspect_quantize = !self.inspect_quantize;
            if let Some(path) = &self.settings_path
                && let Err(error) = settings::save_inspect_quantize(path, self.inspect_quantize)
            {
                self.notice = format!("Could not save Quantize: {error}");
            }
        }
    }

    fn inspect_cursor_label(&self) -> String {
        let beat = self.detail.as_ref().map_or_else(
            || "Beat unavailable".into(),
            |detail| {
                if detail.beat_grid.is_empty() {
                    "No beat grid".into()
                } else if let Some((number, bar_beat)) = detail.beat_grid.beat_at(self.position) {
                    format!("Beat {number} ({bar_beat}/4)")
                } else {
                    "Outside beat grid".into()
                }
            },
        );
        format!(
            "{beat} · {}",
            crate::cues::timestamp((self.position.max(0.0) * 1000.0).round() as u32)
        )
    }

    pub(super) fn paint_inspect_cursor(
        &mut self,
        painter: &Painter,
        rect: Rect,
        range: std::ops::Range<f64>,
        detailed: bool,
    ) {
        if range.end <= range.start
            || !self.position.is_finite()
            || !(range.start..=range.end).contains(&self.position)
        {
            return;
        }
        let painter = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        let rect = Rect::from_min_max(rect.min + vec2(0.0, 17.0), rect.max - vec2(0.0, 3.0));
        let x = rect.left()
            + ((self.position - range.start) / (range.end - range.start)) as f32 * rect.width();
        painter.vline(x, rect.y_range(), Stroke::new(2.0, TEXT));
        if detailed {
            let galley =
                painter.layout_no_wrap(self.inspect_cursor_label(), FontId::monospace(13.0), TEXT);
            let size = galley.size() + vec2(8.0, 6.0);
            let left = if x + 6.0 + size.x <= rect.right() - 4.0 {
                x + 6.0
            } else {
                x - 6.0 - size.x
            };
            let left = left.clamp(
                rect.left() + 4.0,
                (rect.right() - size.x - 4.0).max(rect.left() + 4.0),
            );
            let badge = Rect::from_min_size(pos2(left, rect.top() + 1.0), size);
            painter.rect_filled(badge, 4, BG);
            painter.rect_stroke(badge, 4, Stroke::new(1.0, LINE), StrokeKind::Inside);
            painter.galley(badge.min + vec2(4.0, 3.0), galley, TEXT);
            self.hit("Inspect cursor label", badge);
        }
    }
}
