// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

use super::tag_widgets::{GAP, TILE_HEIGHT};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Scope {
    #[default]
    All,
    Selected,
    Category(u32),
}

#[derive(Default)]
pub(super) struct State {
    scope: Scope,
    last_filter: Option<(Scope, String, usize)>,
}

impl App {
    pub(super) fn track_tag_picker(
        &mut self,
        ui: &mut Ui,
        manager: &mut Manager,
        request: &mut Option<Request>,
    ) {
        let Some(draft) = &mut manager.track else {
            return;
        };
        // Reserve the footer before drawing either independently scrolling list.
        Panel::bottom("track-tag-actions")
            .exact_size(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(PANEL))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(!draft.changed(), |ui| {
                        if self.edit_button(ui, "Manage tags…", 166.0, false) {
                            manager.view = View::Tags;
                            *request = Some(Request::Catalog);
                        }
                    })
                    .response
                    .on_hover_text("Save or cancel track edits before managing tag definitions.");
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let save = ui
                            .add_enabled_ui(draft.changed(), |ui| {
                                theme::primary(ui, "Save track", vec2(140.0, 48.0))
                            })
                            .inner;
                        self.hit("Edit Save track", save.rect);
                        if save.clicked() {
                            *request = Some(draft.request());
                        }
                        if self.edit_button(ui, "Cancel edits", 130.0, false) {
                            *draft = Draft::new(draft.before.clone());
                        }
                    });
                });
            });
        ui.add(Label::new(RichText::new(&draft.before.track.title).size(18.0)).truncate());
        ui.horizontal(|ui| {
            if self.edit_button(ui, "Rating & color", 160.0, false) {
                manager.view = View::Quick;
            }
            self.edit_button(ui, "Track tags", 130.0, true);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!(
                        "{} selected{}",
                        draft.tags.len(),
                        if draft.changed() { " · Unsaved" } else { "" }
                    ))
                    .size(14.0)
                    .color(if draft.changed() { ORANGE } else { MUTED }),
                );
            });
        });
        self.tag_search(ui, &mut manager.query, &mut manager.typing, "Search tags…");

        let categories = tag_widgets::category_ids(&manager.catalog);
        if let Scope::Category(id) = manager.tag_picker.scope
            && !categories.contains(&id)
        {
            manager.tag_picker.scope = Scope::All;
        }
        let rail_id = ui.make_persistent_id(("track-tag-categories", draft.before.track.id));
        let grid_id = ui.make_persistent_id(("track-tag-grid", draft.before.track.id));
        if manager.tag_picker.last_filter.is_none() {
            track_scroll::set_offset(ui.ctx(), rail_id, 0.0);
        }
        let height = (ui.available_height() - GAP).max(0.0);
        if height < 1.0 {
            return;
        }
        let rail_width = (ui.available_width() * 0.24).clamp(168.0, 200.0);
        let grid_width = (ui.available_width() - rail_width - GAP).max(1.0);
        let columns = ((grid_width + GAP) / (208.0 + GAP)).floor().clamp(1.0, 4.0) as usize;
        ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
        // A visible thumb makes overflow discoverable without mouse hover.
        ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                vec2(rail_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    track_scroll::show_rows(
                        ui,
                        rail_id,
                        height,
                        48.0,
                        categories.len() + 2,
                        |ui, rows| {
                            for row in rows {
                                let (scope, title, count) = match row {
                                    0 => (Scope::All, "All tags", manager.catalog.tags.len()),
                                    1 => (Scope::Selected, "Selected", draft.tags.len()),
                                    _ => {
                                        let id = categories[row - 2];
                                        (
                                            Scope::Category(id),
                                            manager.catalog.category(id),
                                            manager
                                                .catalog
                                                .tags
                                                .iter()
                                                .filter(|t| t.category_id == id)
                                                .count(),
                                        )
                                    }
                                };
                                let response = tag_widgets::category_row(
                                    ui,
                                    title,
                                    count,
                                    manager.tag_picker.scope == scope,
                                );
                                self.hit(
                                    &format!(
                                        "Tag category {}",
                                        match scope {
                                            Scope::All => "all".into(),
                                            Scope::Selected => "selected".into(),
                                            Scope::Category(id) => id.to_string(),
                                        }
                                    ),
                                    response.rect,
                                );
                                if response.clicked() {
                                    manager.tag_picker.scope = scope;
                                }
                            }
                        },
                    );
                },
            );
            ui.allocate_ui_with_layout(
                vec2(grid_width, height),
                Layout::top_down(Align::Min),
                |ui| {
                    let query = manager.query.trim().to_lowercase();
                    let filter = (manager.tag_picker.scope, query.clone(), columns);
                    if manager.tag_picker.last_filter.as_ref() != Some(&filter) {
                        track_scroll::set_offset(ui.ctx(), grid_id, 0.0);
                        manager.tag_picker.last_filter = Some(filter);
                    }
                    let mut tags: Vec<_> = manager
                        .catalog
                        .tags
                        .iter()
                        .filter(|tag| {
                            let in_scope = match manager.tag_picker.scope {
                                Scope::All => true,
                                Scope::Selected => draft.tags.contains(&tag.id),
                                Scope::Category(id) => tag.category_id == id,
                            };
                            in_scope
                                && (query.is_empty()
                                    || tag.name.to_lowercase().contains(&query)
                                    || manager
                                        .catalog
                                        .category(tag.category_id)
                                        .to_lowercase()
                                        .contains(&query))
                        })
                        .collect();
                    tags.sort_by_key(|tag| {
                        (
                            tag.name.to_lowercase(),
                            manager.catalog.category(tag.category_id).to_lowercase(),
                            tag.id,
                        )
                    });
                    if tags.is_empty() {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(if manager.catalog.tags.is_empty() {
                                "No tags yet. Use Manage tags to create some."
                            } else if !query.is_empty() {
                                "No tags match this search."
                            } else if manager.tag_picker.scope == Scope::Selected {
                                "No tags selected. Choose All tags to add some."
                            } else {
                                "No tags in this category."
                            })
                            .color(MUTED),
                        );
                        return;
                    }
                    track_scroll::show_rows(
                        ui,
                        grid_id,
                        height,
                        TILE_HEIGHT,
                        tags.len().div_ceil(columns),
                        |ui, rows| {
                            let width = (ui.available_width() - GAP * (columns - 1) as f32)
                                / columns as f32;
                            for row in rows {
                                ui.horizontal(|ui| {
                                    for tag in tags.iter().skip(row * columns).take(columns) {
                                        let response = tag_widgets::tile(
                                            ui,
                                            &tag.name,
                                            manager.catalog.category(tag.category_id),
                                            width,
                                            Some(draft.tags.contains(&tag.id)),
                                        );
                                        self.hit(&format!("Track tag {}", tag.id), response.rect);
                                        if response.clicked() && !draft.tags.remove(&tag.id) {
                                            draft.tags.insert(tag.id);
                                        }
                                    }
                                });
                            }
                        },
                    );
                },
            );
        });
    }
}
