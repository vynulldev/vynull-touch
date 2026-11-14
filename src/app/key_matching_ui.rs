// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::matching::KeyRules;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct Picker {
    error: String,
}

impl App {
    pub(super) fn key_matching_button(&mut self, ui: &mut Ui, label: &str, width: f32) {
        let response = theme::button(ui, label, vec2(width, 48.0), false);
        self.hit("Key matching", response.rect);
        if response.clicked() {
            self.keyboard = None;
            self.library_match_picker = false;
            self.key_matching_picker = Some(Picker::default());
        }
    }

    fn set_key_matching(&mut self, picker: &mut Picker, rules: KeyRules) {
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_key_matching(path, rules)
        {
            picker.error = format!("Could not save key matching: {error}");
            return;
        }
        self.matching.key_rules = rules;
        self.matching.dirty = true;
        picker.error.clear();
    }

    pub(super) fn key_matching_modal(&mut self, ctx: &Context) {
        let Some(mut picker) = self.key_matching_picker.take() else {
            return;
        };
        let mut close = false;
        let modal = Modal::new(Id::unique("key-matching")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(580.0));
            ui.heading("Key matching");
            ui.label("Same key always matches. Choose its compatible neighbors.");
            let mut rules = self.matching.key_rules;
            for (name, label, example, value) in [
                ("Relative", "Relative A/B", "3A to 3B", &mut rules.relative),
                ("Next", "Next +1", "3A to 4A", &mut rules.next),
                ("Previous", "Previous −1", "3A to 2A", &mut rules.previous),
            ] {
                let response = key_rule_toggle(ui, label, example, *value);
                self.hit(&format!("Key rule {name}"), response.rect);
                if response.clicked() {
                    *value = !*value;
                }
            }
            if rules != self.matching.key_rules {
                self.set_key_matching(&mut picker, rules);
            }
            let rules = self.matching.key_rules;
            let example = ["3A", "3B", "4A", "2A"].into_iter()
                .filter(|key| rules.relation("3A", key).is_some()).collect::<Vec<_>>().join(", ");
            ui.label(RichText::new(format!("Example: 3A matches {example}")).color(GREEN));
            ui.label(RichText::new("Applies to library highlighting and Match a player. Steps wrap between 12 and 1.").size(13.0).color(MUTED));
            ui.label(RichText::new("Changes save automatically on this screen.").size(13.0).color(MUTED));
            if !picker.error.is_empty() {
                ui.add(Label::new(RichText::new(&picker.error).size(13.0).color(ORANGE)).wrap());
            }
            ui.horizontal(|ui| {
                let defaults = theme::button(ui, "Restore defaults", vec2(180.0, 48.0), false);
                self.hit("Key rules defaults", defaults.rect);
                if defaults.clicked() { self.set_key_matching(&mut picker, KeyRules::default()); }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let done = theme::primary(ui, "Done", vec2(112.0, 48.0));
                    self.hit("Key rules done", done.rect);
                    close = done.clicked();
                });
            });
        });
        self.hit("Key rules dialog", modal.response.rect);
        if !close && !modal.should_close() {
            self.key_matching_picker = Some(picker);
        }
    }
}

fn key_rule_toggle(ui: &mut Ui, label: &str, example: &str, enabled: bool) -> Response {
    let response = ui.add_sized(
        vec2(ui.available_width(), 56.0),
        Button::new("")
            .fill(if enabled { SELECTED } else { CARD })
            .stroke(Stroke::new(1.0, if enabled { BLUE } else { LINE }))
            .corner_radius(2),
    );
    response.widget_info(|| WidgetInfo::selected(Role::CheckBox, true, enabled, label));
    let rect = response.rect;
    let p = ui.painter();
    let text = rect
        .shrink2(vec2(14.0, 0.0))
        .with_max_x(rect.right() - 120.0);
    theme::text(p, text.with_max_y(rect.top() + 31.0), label, 17.0, TEXT);
    theme::text(p, text.with_min_y(rect.top() + 28.0), example, 13.0, MUTED);
    let switch =
        Rect::from_center_size(pos2(rect.right() - 38.0, rect.center().y), vec2(48.0, 26.0));
    p.text(
        switch.left_center() - vec2(12.0, 0.0),
        Align2::RIGHT_CENTER,
        if enabled { "On" } else { "Off" },
        FontId::proportional(16.0),
        if enabled { CYAN } else { MUTED },
    );
    p.rect_filled(switch, 13, if enabled { BLUE } else { LINE });
    p.circle_filled(
        pos2(
            if enabled {
                switch.right() - 13.0
            } else {
                switch.left() + 13.0
            },
            switch.center().y,
        ),
        10.0,
        TEXT,
    );
    response
}
