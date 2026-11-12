// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::library::{self, Filters, Sort};
use std::collections::BTreeMap;

#[cfg(test)]
mod cue_tests;
#[cfg(test)]
mod match_tests;
mod match_ui;
#[cfg(test)]
mod preview_tests;
mod preview_ui;
mod table;

const TABS: [&str; 6] = ["BPM", "Key", "Genre", "Rating", "Color", "Tags"];

pub(super) struct FilterDraft {
    filters: Filters,
    min: String,
    max: String,
    tab: usize,
    editing: usize,
    options: [Vec<(String, String)>; 3],
    last: Option<Filters>,
    count: usize,
    pub dirty: bool,
}

impl FilterDraft {
    fn new(filters: &Filters, tracks: &[Track]) -> Self {
        let options = [0, 1, 2].map(|category| {
            let mut values = BTreeMap::new();
            for track in tracks {
                let names: Vec<_> = match category {
                    0 => vec![&track.key],
                    1 => vec![&track.genre],
                    _ => track.tags.iter().collect(),
                };
                for name in names {
                    if !name.trim().is_empty() {
                        values
                            .entry(library::normalized(name))
                            .or_insert_with(|| name.trim().to_string());
                    }
                }
            }
            let selected = match category {
                0 => &filters.keys,
                1 => &filters.genres,
                _ => &filters.tags,
            };
            for value in selected.iter().filter(|v| !v.is_empty()) {
                values.entry(value.clone()).or_insert_with(|| value.clone());
            }
            let mut values: Vec<_> = values.into_iter().collect();
            if category == 0 {
                values.sort_by(|a, b| library::compare_keys(&a.0, &b.0));
            }
            values.insert(
                0,
                (
                    String::new(),
                    if category == 2 { "Untagged" } else { "Unknown" }.into(),
                ),
            );
            values
        });
        Self {
            filters: filters.clone(),
            min: filters.min_bpm.map_or(String::new(), |v| v.to_string()),
            max: filters.max_bpm.map_or(String::new(), |v| v.to_string()),
            tab: 0,
            editing: 0,
            options,
            last: None,
            count: 0,
            dirty: true,
        }
    }

    fn parsed(&self) -> Result<Filters, &'static str> {
        let (min_bpm, max_bpm) = library::bpm_range(&self.min, &self.max)?;
        Ok(Filters {
            min_bpm,
            max_bpm,
            ..self.filters.clone()
        })
    }

    fn active(&self, tab: usize) -> bool {
        match tab {
            0 => !self.min.is_empty() || !self.max.is_empty(),
            1 => !self.filters.keys.is_empty(),
            2 => !self.filters.genres.is_empty(),
            3 => self.filters.rating.is_some(),
            4 => !self.filters.colors.is_empty(),
            _ => !self.filters.tags.is_empty(),
        }
    }
}

impl App {
    pub(super) fn collection_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let clear_width = if self.query.is_empty() { 0.0 } else { 56.0 };
            let search_width = (ui.available_width() - clear_width - 360.0).max(64.0);
            let search = ui.add_sized(
                vec2(search_width, 48.0),
                TextEdit::singleline(&mut self.query)
                    .id_salt("library-search")
                    .hint_text("Search…")
                    .background_color(CARD)
                    .margin(vec2(12.0, 14.0)),
            );
            self.hit("Search", search.rect);
            if search.clicked() {
                self.keyboard = Some(Field::Search);
            }
            if search.changed() {
                self.filter();
            }
            if clear_width > 0.0 {
                let clear = theme::button(ui, "×", vec2(48.0, 48.0), false);
                self.hit("Clear search", clear.rect);
                if clear.clicked() {
                    self.query.clear();
                    self.filter();
                }
            }
            self.library_match_button(ui);
            let active = self.filters.count();
            let label = if active == 0 {
                "Filters".into()
            } else {
                format!("Filters · {active}")
            };
            let filters = theme::button(ui, &label, vec2(100.0, 48.0), active > 0);
            self.hit("Library filters", filters.rect);
            if filters.clicked() {
                self.keyboard = None;
                self.filter_dialog = Some(FilterDraft::new(&self.filters, &self.all_tracks));
            }
            let sort = ui.add_sized(
                vec2(124.0, 48.0),
                Button::new(RichText::new(self.order.label()).size(14.0))
                    .fill(CARD)
                    .corner_radius(8),
            );
            self.hit("Library sort", sort.rect);
            if sort.clicked() {
                self.keyboard = None;
                self.sort_dialog = Some(self.order);
            }
        });
    }

    fn apply_order(&mut self, order: Order) {
        self.order = order;
        self.filter();
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_order(path, order)
        {
            self.notice = format!("Sort changed, but settings could not be saved: {error}");
        }
    }

    pub(super) fn filter_summary(&mut self, ui: &mut Ui) {
        if self.filters.count() == 0 {
            return;
        }
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(
                vec2((ui.available_width() - 104.0).max(40.0), 48.0),
                Sense::hover(),
            );
            theme::text(ui.painter(), rect, self.filters.summary(), 12.0, ORANGE);
            let clear = theme::button(ui, "Clear", vec2(96.0, 48.0), false);
            self.hit("Clear filters", clear.rect);
            if clear.clicked() {
                self.apply_filters(Filters::default());
            }
        });
    }

    fn apply_filters(&mut self, filters: Filters) {
        if self.filters != filters {
            self.filters = filters;
            self.filter_revision = self.filter_revision.wrapping_add(1);
            self.filter();
        }
    }

    pub(super) fn sort_modal(&mut self, ctx: &Context) {
        let Some(mut order) = self.sort_dialog else {
            return;
        };
        let mut close = false;
        let modal = Modal::new(Id::unique("library-sort")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(600.0));
            ui.heading("Sort tracks");
            let width = (ui.available_width() - 16.0) / 3.0;
            for row in Sort::ALL.chunks(3) {
                ui.horizontal(|ui| {
                    for &field in row {
                        let response = theme::button(
                            ui,
                            field.label(),
                            vec2(width, 48.0),
                            order.field == field,
                        );
                        self.hit(&format!("Sort {}", field.label()), response.rect);
                        if response.clicked() {
                            order.field = field;
                        }
                    }
                });
            }
            ui.add_enabled_ui(order.field != Sort::Original, |ui| {
                let direction_width = (ui.available_width() - 8.0) / 2.0;
                ui.horizontal(|ui| {
                    for (descending, label) in [(false, "Ascending"), (true, "Descending")] {
                        let button = theme::button(
                            ui,
                            label,
                            vec2(direction_width, 48.0),
                            order.descending == descending,
                        );
                        self.hit(&format!("Sort {label}"), button.rect);
                        if button.clicked() {
                            order.descending = descending;
                        }
                    }
                });
            });
            ui.label(
                RichText::new(if order.field == Sort::Original {
                    "Original keeps the collection or playlist order."
                } else {
                    "Unknown values stay at the end. Equal values keep source order."
                })
                .size(12.0)
                .color(MUTED),
            );
            ui.horizontal(|ui| {
                let cancel = theme::button(ui, "Cancel", vec2(120.0, 48.0), false);
                self.hit("Sort cancel", cancel.rect);
                if cancel.clicked() {
                    close = true;
                }
                let apply = theme::primary(ui, "Apply sort", vec2(ui.available_width(), 48.0));
                self.hit("Sort apply", apply.rect);
                if apply.clicked() {
                    if order.field == Sort::Original {
                        order.descending = false;
                    }
                    self.apply_order(order);
                    close = true;
                }
            });
        });
        self.hit("Sort dialog", modal.response.rect);
        self.sort_dialog = (!close).then_some(order);
    }

    pub(super) fn filter_modal(&mut self, ctx: &Context) {
        let Some(mut draft) = self.filter_dialog.take() else {
            return;
        };
        let mut close = false;
        let modal = Modal::new(Id::unique("library-filters")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(680.0));
            ui.heading("Filter tracks");
            let tab_width = (ui.available_width() - 40.0) / 6.0;
            ui.horizontal(|ui| {
                for (tab, name) in TABS.iter().enumerate() {
                    let label = format!("{name}{}", if draft.active(tab) { " •" } else { "" });
                    let response =
                        theme::button(ui, &label, vec2(tab_width, 48.0), draft.tab == tab);
                    self.hit(&format!("Filter tab {name}"), response.rect);
                    if response.clicked() {
                        draft.tab = tab;
                    }
                }
            });
            ui.allocate_ui(vec2(ui.available_width(), 224.0), |ui| {
                if draft.tab == 0 {
                    self.bpm_controls(ui, &mut draft);
                } else {
                    self.filter_choices(ui, &mut draft);
                }
            });
            let parsed = draft.parsed();
            match &parsed {
                Ok(filters) => {
                    if draft.dirty || draft.last.as_ref() != Some(filters) {
                        let query = self.query.to_lowercase();
                        let words: Vec<_> = query.split_whitespace().collect();
                        draft.count = self
                            .tracks
                            .iter()
                            .zip(&self.search_text)
                            .filter(|(track, search)| {
                                filters.matches(track)
                                    && words.iter().all(|word| search.contains(word))
                            })
                            .count();
                        draft.last = Some(filters.clone());
                        draft.dirty = false;
                    }
                    ui.add(
                        Label::new(
                            RichText::new(format!(
                                "{} of {} tracks · current playlist and search included",
                                draft.count,
                                self.tracks.len()
                            ))
                            .size(13.0)
                            .color(MUTED),
                        )
                        .truncate(),
                    );
                }
                Err(error) => {
                    ui.label(RichText::new(*error).size(13.0).color(ORANGE));
                }
            }
            ui.horizontal(|ui| {
                let clear = theme::button(ui, "Clear all", vec2(112.0, 48.0), false);
                self.hit("Filters reset", clear.rect);
                if clear.clicked() {
                    draft.filters = Filters::default();
                    draft.min.clear();
                    draft.max.clear();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let apply = ui
                        .add_enabled_ui(parsed.is_ok(), |ui| {
                            theme::primary(ui, "Apply filters", vec2(144.0, 48.0))
                        })
                        .inner;
                    self.hit("Filters apply", apply.rect);
                    if apply.clicked()
                        && let Ok(filters) = parsed
                    {
                        self.apply_filters(filters);
                        close = true;
                    }
                    let cancel = theme::button(ui, "Cancel", vec2(112.0, 48.0), false);
                    self.hit("Filters cancel", cancel.rect);
                    if cancel.clicked() {
                        close = true;
                    }
                });
            });
        });
        self.hit("Filters dialog", modal.response.rect);
        if !close {
            self.filter_dialog = Some(draft);
        }
    }

    fn bpm_controls(&mut self, ui: &mut Ui, draft: &mut FilterDraft) {
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(200.0, 216.0), Layout::top_down(Align::Min), |ui| {
                for (index, label) in ["Minimum BPM", "Maximum BPM"].into_iter().enumerate() {
                    ui.label(
                        RichText::new(label)
                            .size(13.0)
                            .color(if draft.editing == index {
                                ORANGE
                            } else {
                                MUTED
                            }),
                    );
                    let value = if index == 0 {
                        &mut draft.min
                    } else {
                        &mut draft.max
                    };
                    let field = ui.add_sized(
                        vec2(200.0, 48.0),
                        TextEdit::singleline(value).hint_text("Any").char_limit(8),
                    );
                    self.hit(label, field.rect);
                    if field.clicked() || field.has_focus() {
                        draft.editing = index;
                    }
                }
                let clear = theme::button(ui, "Clear field", vec2(200.0, 48.0), false);
                self.hit("BPM clear", clear.rect);
                if clear.clicked() {
                    if draft.editing == 0 {
                        draft.min.clear();
                    } else {
                        draft.max.clear();
                    }
                }
            });
            ui.vertical(|ui| {
                let width = (ui.available_width() - 16.0) / 3.0;
                for row in [
                    ["7", "8", "9"],
                    ["4", "5", "6"],
                    ["1", "2", "3"],
                    [".", "0", "Backspace"],
                ] {
                    ui.horizontal(|ui| {
                        for key in row {
                            let button = theme::button(ui, key, vec2(width, 48.0), false);
                            self.hit(&format!("BPM key {key}"), button.rect);
                            if button.clicked() {
                                let value = if draft.editing == 0 {
                                    &mut draft.min
                                } else {
                                    &mut draft.max
                                };
                                if key == "Backspace" {
                                    value.pop();
                                } else if value.len() < 8 {
                                    value.push_str(key);
                                }
                            }
                        }
                    });
                }
            });
        });
    }

    fn filter_choices(&mut self, ui: &mut Ui, draft: &mut FilterDraft) {
        let tab = draft.tab;
        let mut options = vec![(
            "any".to_string(),
            format!("Any {}", TABS[tab].to_lowercase()),
        )];
        match tab {
            1 | 2 | 5 => options.extend(
                draft.options[match tab {
                    1 => 0,
                    2 => 1,
                    _ => 2,
                }]
                .clone(),
            ),
            3 => options.extend((0..=5).map(|n| {
                (
                    n.to_string(),
                    if n == 0 {
                        "Unrated".into()
                    } else {
                        format!("{n} stars or more")
                    },
                )
            })),
            4 => options.extend(
                library::COLORS
                    .iter()
                    .enumerate()
                    .map(|(n, name)| (n.to_string(), name.to_string())),
            ),
            _ => (),
        }
        let id = ui.make_persistent_id(("filter-options", tab));
        track_scroll::show_rows(
            ui,
            id,
            224.0,
            48.0,
            options.len().div_ceil(3),
            |ui, range| {
                let width = (ui.available_width() - 16.0) / 3.0;
                for row in range {
                    ui.horizontal(|ui| {
                        for (index, (value, label)) in
                            options.iter().enumerate().skip(row * 3).take(3)
                        {
                            let any = index == 0;
                            let active = if any {
                                !draft.active(tab)
                            } else {
                                match tab {
                                    1 => draft.filters.keys.contains(value),
                                    2 => draft.filters.genres.contains(value),
                                    3 => draft.filters.rating == value.parse().ok(),
                                    4 => value
                                        .parse()
                                        .is_ok_and(|n| draft.filters.colors.contains(&n)),
                                    _ => draft.filters.tags.contains(value),
                                }
                            };
                            let (rect, response) =
                                ui.allocate_exact_size(vec2(width, 48.0), Sense::click());
                            ui.painter()
                                .rect_filled(rect, 8, if active { SELECTED } else { CARD });
                            theme::text(
                                ui.painter(),
                                rect.shrink2(vec2(10.0, 0.0)),
                                label,
                                15.0,
                                if active { ORANGE } else { TEXT },
                            );
                            self.hit(&format!("Filter {} {label}", TABS[tab]), rect);
                            if response.clicked() {
                                match tab {
                                    3 => {
                                        draft.filters.rating =
                                            if any { None } else { value.parse().ok() }
                                    }
                                    4 => {
                                        if any {
                                            draft.filters.colors.clear();
                                        } else if let Ok(color) = value.parse() {
                                            toggle(&mut draft.filters.colors, color);
                                        }
                                    }
                                    _ => {
                                        let list = match tab {
                                            1 => &mut draft.filters.keys,
                                            2 => &mut draft.filters.genres,
                                            _ => &mut draft.filters.tags,
                                        };
                                        if any {
                                            list.clear();
                                        } else {
                                            toggle(list, value.clone());
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            },
        );
    }
}

fn toggle<T: PartialEq>(values: &mut Vec<T>, value: T) {
    if values.contains(&value) {
        values.retain(|v| *v != value);
    } else {
        values.push(value);
    }
}

#[cfg(test)]
mod tests;
