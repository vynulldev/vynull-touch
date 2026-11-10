// SPDX-License-Identifier: GPL-3.0-or-later
use super::tag_widgets::{GAP, TILE_HEIGHT};
use super::*;

mod browser;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub(super) struct State {
    category: Option<u32>,
    tag_filter: Option<(Option<u32>, String, usize)>,
    category_filter: Option<(String, usize)>,
    choice_initialized: bool,
    category_from_tags: bool,
}

impl App {
    pub(super) fn catalog_controls(
        &mut self,
        ui: &mut Ui,
        manager: &mut Manager,
        request: &mut Option<Request>,
    ) {
        if matches!(manager.view, View::Tags | View::Categories) {
            manager.catalog_browser.choice_initialized = false;
            self.catalog_browser(ui, manager, request);
            return;
        }
        let mut next = None;
        let category_return = if manager.catalog_browser.category_from_tags {
            View::Tags
        } else {
            View::Categories
        };
        match &mut manager.view {
            View::TagForm {
                before,
                name,
                category,
            } => {
                Panel::bottom("tag-form-actions")
                    .exact_size(48.0).resizable(false).show_separator_line(false)
                    .frame(Frame::new().fill(PANEL))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(tag) = before.as_ref()
                                && self.edit_button(ui, "Delete tag…", 150.0, false) {
                                manager.confirm = Some((format!("Delete ‘{}’ from every track? This also removes the tag definition.", tag.name), Request::DeleteTag(tag.clone())));
                                manager.after = Some(View::Tags);
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let save = ui.add_enabled_ui(editing::name(name).is_ok(), |ui| {
                                    theme::primary(ui, "Save tag", vec2(140.0, 48.0))
                                }).inner;
                                self.hit("Edit Save tag", save.rect);
                                if save.clicked() {
                                    *request = Some(Request::Tag {
                                        before: before.clone(), name: name.clone(),
                                        category: manager.catalog.categories.iter().find(|c| c.id == *category).cloned(),
                                    });
                                    manager.after = Some(View::Tags);
                                }
                                if self.edit_button(ui, "Cancel", 100.0, false) {
                                    next = Some(View::Tags);
                                }
                            });
                        });
                    });
                ui.label(
                    RichText::new(if before.is_some() {
                        "Changes affect this tag everywhere in the library."
                    } else {
                        "Create a reusable library tag, then assign it to tracks."
                    })
                    .color(MUTED),
                );
                self.edit_name(ui, name, &mut manager.typing);
                ui.label("Category");
                let mut categories: Vec<_> =
                    manager.catalog.categories.iter().map(|c| c.id).collect();
                categories.sort_by_key(|id| (manager.catalog.category(*id).to_lowercase(), *id));
                categories.insert(0, 0);
                let columns = ((ui.available_width() + GAP) / (208.0 + GAP))
                    .floor()
                    .clamp(1.0, 4.0) as usize;
                let height = (ui.available_height() - GAP).max(1.0);
                let id = ui.make_persistent_id("tag-form-categories");
                if !manager.catalog_browser.choice_initialized {
                    let row =
                        categories.iter().position(|id| id == category).unwrap_or(0) / columns;
                    track_scroll::set_offset(ui.ctx(), id, row as f32 * (TILE_HEIGHT + 6.0));
                    manager.catalog_browser.choice_initialized = true;
                }
                ui.spacing_mut().item_spacing.x = GAP;
                ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
                track_scroll::show_rows(
                    ui,
                    id,
                    height,
                    TILE_HEIGHT,
                    categories.len().div_ceil(columns),
                    |ui, rows| {
                        let width =
                            (ui.available_width() - GAP * (columns - 1) as f32) / columns as f32;
                        for row in rows {
                            ui.horizontal(|ui| {
                                for id in categories.iter().skip(row * columns).take(columns) {
                                    let title = manager.catalog.category(*id);
                                    let response = tag_widgets::tile(
                                        ui,
                                        title,
                                        if *id == 0 {
                                            "No category"
                                        } else {
                                            "Tag category"
                                        },
                                        width,
                                        Some(*category == *id),
                                    );
                                    self.hit(&format!("Tag form category {id}"), response.rect);
                                    if response.clicked() {
                                        *category = *id;
                                    }
                                }
                            });
                        }
                    },
                );
            }
            View::CategoryForm { before, name } => {
                Panel::bottom("category-form-actions")
                    .exact_size(48.0).resizable(false).show_separator_line(false)
                    .frame(Frame::new().fill(PANEL))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            if let Some(category) = before.as_ref()
                                && self.edit_button(ui, "Delete category…", 190.0, false) {
                                let tags: Vec<_> = manager.catalog.tags.iter().filter(|t| t.category_id == category.id).cloned().collect();
                                manager.confirm = Some((format!("Delete ‘{}’? Its {} tags will move to Uncategorized and stay assigned to their tracks.", category.name, tags.len()), Request::DeleteCategory { before: category.clone(), tags }));
                                manager.after = Some(category_return.clone());
                            }
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let save = ui.add_enabled_ui(editing::name(name).is_ok(), |ui| {
                                    theme::primary(ui, "Save category", vec2(170.0, 48.0))
                                }).inner;
                                self.hit("Edit Save category", save.rect);
                                if save.clicked() {
                                    *request = Some(Request::Category { before: before.clone(), name: name.clone() });
                                    manager.after = Some(category_return.clone());
                                }
                                if self.edit_button(ui, "Cancel", 100.0, false) { next = Some(category_return.clone()); }
                            });
                        });
                    });
                ui.label(
                    RichText::new(if before.is_some() {
                        "Rename this category for every tag that uses it."
                    } else {
                        "Group related tags in a reusable category."
                    })
                    .color(MUTED),
                );
                self.edit_name(ui, name, &mut manager.typing);
            }
            _ => (),
        }
        if let Some(next) = next {
            manager.view = next;
        }
    }

    pub(super) fn edit_name(&mut self, ui: &mut Ui, name: &mut String, typing: &mut bool) {
        let response = ui.add_sized(
            vec2(ui.available_width(), 48.0),
            TextEdit::singleline(name)
                .hint_text("Name…")
                .char_limit(128),
        );
        self.hit("Edit Name", response.rect);
        if response.clicked() {
            *typing = true;
        }
    }
}
