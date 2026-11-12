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
            for (name, label, value) in [
                ("Relative", "Relative A/B   ·   3A to 3B", &mut rules.relative),
                ("Next", "Next +1   ·   3A to 4A", &mut rules.next),
                ("Previous", "Previous −1   ·   3A to 2A", &mut rules.previous),
            ] {
                let response = ui.add(Checkbox::new(value, RichText::new(label).size(17.0)));
                self.hit(&format!("Key rule {name}"), response.rect);
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
