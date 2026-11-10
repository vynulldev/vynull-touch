// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(super) fn catalog_browser(
        &mut self,
        ui: &mut Ui,
        manager: &mut Manager,
        request: &mut Option<Request>,
    ) {
        let showing_categories = matches!(manager.view, View::Categories);
        let mut next = None;
        let mut categories = tag_widgets::category_ids(&manager.catalog);
        if !categories.contains(&0) {
            categories.push(0);
            categories.sort_by_key(|id| (manager.catalog.category(*id).to_lowercase(), *id));
        }
        if manager
            .catalog_browser
            .category
            .is_some_and(|id| !categories.contains(&id))
        {
            manager.catalog_browser.category = None;
        }
        Panel::bottom("catalog-browser-actions")
            .exact_size(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(PANEL))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if showing_categories {
                        if self.edit_button(ui, "‹ Tags", 110.0, false) {
                            manager.query.clear();
                            next = Some(View::Tags);
                        }
                    } else {
                        if let Some(track) = &manager.origin
                            && self.edit_button(ui, "‹ Track", 110.0, false)
                        {
                            next = Some(View::TrackTags);
                            *request = Some(Request::Track(track.clone()));
                        }
                        if self.edit_button(ui, "Categories", 140.0, false) {
                            manager.query.clear();
                            next = Some(View::Categories);
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let label = if showing_categories {
                            "New category"
                        } else {
                            "New tag"
                        };
                        let new = theme::primary(
                            ui,
                            label,
                            vec2(if showing_categories { 170.0 } else { 140.0 }, 48.0),
                        );
                        self.hit(&format!("Edit {label}"), new.rect);
                        if new.clicked() {
                            manager.catalog_browser.category_from_tags = false;
                            next = Some(if showing_categories {
                                View::CategoryForm {
                                    before: None,
                                    name: String::new(),
                                }
                            } else {
                                View::TagForm {
                                    before: None,
                                    name: String::new(),
                                    category: manager
                                        .catalog_browser
                                        .category
                                        .filter(|id| {
                                            manager.catalog.categories.iter().any(|c| c.id == *id)
                                        })
                                        .unwrap_or(0),
                                }
                            });
                        }
                        if !showing_categories
                            && let Some(category) = manager
                                .catalog
                                .categories
                                .iter()
                                .find(|c| Some(c.id) == manager.catalog_browser.category)
                            && self.edit_button(ui, "Edit category…", 180.0, false)
                        {
                            manager.catalog_browser.category_from_tags = true;
                            next = Some(View::CategoryForm {
                                before: Some(category.clone()),
                                name: category.name.clone(),
                            });
                        }
                    });
                });
            });
        ui.label(
            RichText::new(if showing_categories {
                "Tap a category to edit it. Uncategorized is permanent."
            } else {
                "Tap a tag to rename it, change its category, or delete it."
            })
            .color(MUTED),
        );
        self.tag_search(
            ui,
            &mut manager.query,
            &mut manager.typing,
            if showing_categories {
                "Search categories…"
            } else {
                "Search tags…"
            },
        );
        let height = (ui.available_height() - GAP).max(1.0);
        ui.spacing_mut().item_spacing = vec2(GAP, 6.0);
        ui.spacing_mut().scroll.dormant_handle_opacity = 1.0;
        if showing_categories {
            let query = manager.query.trim().to_lowercase();
            let mut ids: Vec<_> = manager
                .catalog
                .categories
                .iter()
                .map(|c| c.id)
                .chain([0])
                .filter(|id| {
                    manager
                        .catalog
                        .category(*id)
                        .to_lowercase()
                        .contains(&query)
                })
                .collect();
            ids.sort_by_key(|id| (manager.catalog.category(*id).to_lowercase(), *id));
            let columns = columns(ui.available_width());
            let filter = (query, columns);
            let id = ui.make_persistent_id("catalog-category-grid");
            if manager.catalog_browser.category_filter.as_ref() != Some(&filter) {
                track_scroll::set_offset(ui.ctx(), id, 0.0);
                manager.catalog_browser.category_filter = Some(filter);
            }
            if ids.is_empty() {
                ui.label(RichText::new("No categories match this search.").color(MUTED));
            } else {
                track_scroll::show_rows(
                    ui,
                    id,
                    height,
                    TILE_HEIGHT,
                    ids.len().div_ceil(columns),
                    |ui, rows| {
                        let width =
                            (ui.available_width() - GAP * (columns - 1) as f32) / columns as f32;
                        for row in rows {
                            ui.horizontal(|ui| {
                                for id in ids.iter().skip(row * columns).take(columns) {
                                    let count = manager
                                        .catalog
                                        .tags
                                        .iter()
                                        .filter(|t| t.category_id == *id)
                                        .count();
                                    let subtitle = format!(
                                        "{count} {}{}",
                                        if count == 1 { "tag" } else { "tags" },
                                        if *id == 0 { " · View tags" } else { "" }
                                    );
                                    let response = tag_widgets::tile(
                                        ui,
                                        manager.catalog.category(*id),
                                        &subtitle,
                                        width,
                                        None,
                                    );
                                    self.hit(&format!("Manage category {id}"), response.rect);
                                    if response.clicked() {
                                        if let Some(category) =
                                            manager.catalog.categories.iter().find(|c| c.id == *id)
                                        {
                                            manager.catalog_browser.category_from_tags = false;
                                            next = Some(View::CategoryForm {
                                                before: Some(category.clone()),
                                                name: category.name.clone(),
                                            });
                                        } else {
                                            manager.catalog_browser.category = Some(0);
                                            manager.query.clear();
                                            next = Some(View::Tags);
                                        }
                                    }
                                }
                            });
                        }
                    },
                );
            }
        } else {
            let rail_width = (ui.available_width() * 0.24).clamp(168.0, 200.0);
            let grid_width = (ui.available_width() - rail_width - GAP).max(1.0);
            let rail_id = ui.make_persistent_id("catalog-tag-categories");
            if manager.catalog_browser.tag_filter.is_none() {
                track_scroll::set_offset(ui.ctx(), rail_id, 0.0);
            }
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
                            categories.len() + 1,
                            |ui, rows| {
                                for row in rows {
                                    let category = row.checked_sub(1).map(|i| categories[i]);
                                    let title = category
                                        .map_or("All tags", |id| manager.catalog.category(id));
                                    let count = manager
                                        .catalog
                                        .tags
                                        .iter()
                                        .filter(|t| category.is_none_or(|id| t.category_id == id))
                                        .count();
                                    let response = tag_widgets::category_row(
                                        ui,
                                        title,
                                        count,
                                        manager.catalog_browser.category == category,
                                    );
                                    self.hit(
                                        &format!(
                                            "Manage scope {}",
                                            category.map_or("all".into(), |id| id.to_string())
                                        ),
                                        response.rect,
                                    );
                                    if response.clicked() {
                                        manager.catalog_browser.category = category;
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
                        let columns = columns(grid_width);
                        let filter = (manager.catalog_browser.category, query.clone(), columns);
                        let id = ui.make_persistent_id("catalog-tag-grid");
                        if manager.catalog_browser.tag_filter.as_ref() != Some(&filter) {
                            track_scroll::set_offset(ui.ctx(), id, 0.0);
                            manager.catalog_browser.tag_filter = Some(filter);
                        }
                        let mut tags: Vec<_> = manager
                            .catalog
                            .tags
                            .iter()
                            .filter(|tag| {
                                manager
                                    .catalog_browser
                                    .category
                                    .is_none_or(|id| tag.category_id == id)
                                    && (tag.name.to_lowercase().contains(&query)
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
                            ui.label(
                                RichText::new(if !query.is_empty() {
                                    "No tags match this search."
                                } else {
                                    "No tags here yet. Tap New tag to create one."
                                })
                                .color(MUTED),
                            );
                        } else {
                            track_scroll::show_rows(
                                ui,
                                id,
                                height,
                                TILE_HEIGHT,
                                tags.len().div_ceil(columns),
                                |ui, rows| {
                                    let width = (ui.available_width() - GAP * (columns - 1) as f32)
                                        / columns as f32;
                                    for row in rows {
                                        ui.horizontal(|ui| {
                                            for tag in tags.iter().skip(row * columns).take(columns)
                                            {
                                                let response = tag_widgets::tile(
                                                    ui,
                                                    &tag.name,
                                                    manager.catalog.category(tag.category_id),
                                                    width,
                                                    None,
                                                );
                                                self.hit(
                                                    &format!("Manage tag {}", tag.id),
                                                    response.rect,
                                                );
                                                if response.clicked() {
                                                    next = Some(View::TagForm {
                                                        before: Some((*tag).clone()),
                                                        name: tag.name.clone(),
                                                        category: tag.category_id,
                                                    });
                                                }
                                            }
                                        });
                                    }
                                },
                            );
                        }
                    },
                );
            });
        }
        if let Some(next) = next {
            manager.view = next;
        }
    }
}

fn columns(width: f32) -> usize {
    ((width + GAP) / (208.0 + GAP)).floor().clamp(1.0, 4.0) as usize
}
