// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::browser_settings::{Edit, Snapshot};

#[derive(Default)]
pub(super) struct Editor {
    snapshot: Option<Snapshot>,
    draft: Option<Snapshot>,
    pub pending: bool,
    pub picker: bool,
    reload: bool,
    serial: u64,
    sorts: bool,
    message: String,
}

impl Editor {
    pub(super) fn dirty(&self) -> bool {
        self.snapshot
            .as_ref()
            .zip(self.draft.as_ref())
            .is_some_and(|(saved, draft)| !saved.same_config(draft))
    }
}

struct Row {
    key: String,
    label: String,
    note: &'static str,
    visible: bool,
    locked: bool,
    can_hide: bool,
    sort_id: Option<u32>,
}

impl App {
    pub(super) fn open_browser_settings(&mut self) {
        if !self.browser_settings.pending && !self.browser_settings.dirty() {
            self.browser_settings_request(false);
        }
    }

    fn browser_settings_request(&mut self, save: bool) {
        if self.browser_settings.pending {
            return;
        }
        let edit = if save {
            if self.browser_settings.reload {
                return;
            }
            let Some((before, draft)) = self
                .browser_settings
                .snapshot
                .as_ref()
                .zip(self.browser_settings.draft.as_ref())
            else {
                return;
            };
            if before.same_config(draft) {
                return;
            }
            Some(Box::new(Edit {
                before: before.clone(),
                draft: draft.clone(),
            }))
        } else {
            None
        };
        self.browser_settings.serial += 1;
        let token = self.browser_settings.serial;
        self.browser_settings.picker = false;
        if self.options.demo {
            let saved = self
                .browser_settings
                .snapshot
                .clone()
                .unwrap_or_else(Snapshot::demo);
            let result = edit.map_or_else(
                || Ok(saved.clone()),
                |edit| edit.prepare(&saved).map(|(_, target)| target),
            );
            self.browser_settings_response(token, save, result);
        } else if !self.library_ready {
            self.browser_settings.message = "Connect to Vynull, then tap Reload.".into();
        } else {
            self.browser_settings.pending = self.send(Command::BrowserSettings(token, edit));
            self.browser_settings.message = if self.browser_settings.pending {
                if save {
                    "Saving and reading back browser settings…"
                } else {
                    "Loading browser settings…"
                }
            } else {
                "Could not queue the request. Try again."
            }
            .into();
        }
    }

    pub(super) fn browser_settings_response(
        &mut self,
        token: u64,
        written: bool,
        result: Result<Snapshot, String>,
    ) {
        let editor = &mut self.browser_settings;
        if token != editor.serial {
            return;
        }
        editor.pending = false;
        match result {
            Ok(snapshot) => {
                editor.draft = Some(snapshot.clone());
                editor.snapshot = Some(snapshot);
                editor.reload = false;
                editor.message = if self.options.demo {
                    "Demo browser · changes last for this demo session".into()
                } else if written {
                    "Saved and read back · refresh the CDJ browser to see changes".into()
                } else {
                    "Shown entries appear in this order · required categories stay visible".into()
                };
            }
            Err(error) => {
                editor.reload = true;
                editor.message = error;
            }
        }
    }

    pub(super) fn browser_settings_view(&mut self, ui: &mut Ui) {
        let enabled = !self.browser_settings.pending
            && !self.browser_settings.reload
            && self.browser_settings.draft.is_some();
        ui.horizontal(|ui| {
            for (sorts, title) in [(false, "Categories"), (true, "Sort menu")] {
                let tab = theme::button(
                    ui,
                    title,
                    vec2(120.0, 48.0),
                    self.browser_settings.sorts == sorts,
                );
                self.hit(&format!("Browser {title}"), tab.rect);
                if tab.clicked() {
                    self.browser_settings.sorts = sorts;
                }
            }
            let detail = self
                .browser_settings
                .draft
                .as_ref()
                .and_then(|s| {
                    s.track_detail_fields
                        .iter()
                        .find(|f| f.key == s.track_detail)
                })
                .map_or("…", |f| f.label.as_str());
            let response = ui
                .add_enabled_ui(enabled, |ui| {
                    theme::button(
                        ui,
                        &format!("Detail: {detail}  ›"),
                        vec2((ui.available_width()).min(300.0), 48.0),
                        false,
                    )
                })
                .inner
                .on_hover_text("The secondary track column when the CDJ uses DEFAULT sort");
            self.hit("Browser detail", response.rect);
            if response.clicked() {
                self.browser_settings.picker = true;
            }
        });
        let area = ui.available_rect_before_wrap();
        let footer_height = 70.0;
        let height = (area.height() - footer_height - 8.0).max(0.0);
        let sorts = self.browser_settings.sorts;
        let mut rows = self
            .browser_settings
            .draft
            .as_ref()
            .map(|draft| {
                if sorts {
                    let shown = draft.sorts.iter().filter(|s| s.visible).count();
                    draft
                        .sorts
                        .iter()
                        .map(|s| Row {
                            key: s.id.to_string(),
                            label: s.label.clone(),
                            note: "",
                            visible: s.visible,
                            locked: false,
                            can_hide: shown > 1,
                            sort_id: Some(s.id),
                        })
                        .collect::<Vec<_>>()
                } else {
                    draft
                        .items
                        .iter()
                        .map(|s| Row {
                            key: s.key.clone(),
                            label: s.label.clone(),
                            note: if s.export_only { "USB export only" } else { "" },
                            visible: s.visible,
                            locked: s.locked,
                            can_hide: !s.locked,
                            sort_id: None,
                        })
                        .collect()
                }
            })
            .unwrap_or_default();
        rows.sort_by_key(|r| !r.visible); // Stable: retain order within shown/hidden.
        let shown = rows.iter().filter(|r| r.visible).count();
        let scroll_id = ui.make_persistent_id(("browser-menu", sorts));
        if rows.is_empty() {
            ui.label("Load browser settings from Vynull to edit its menus.");
        } else {
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                track_scroll::show_rows(ui, scroll_id, height, 50.0, rows.len(), |ui, range| {
                    for index in range {
                        let row = &rows[index];
                        ui.push_id((&row.key, sorts), |ui| {
                            Frame::new()
                                .fill(PANEL)
                                .inner_margin(Margin::symmetric(8, 0))
                                .corner_radius(6)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let label_width =
                                            (ui.available_width() - 100.0 - 80.0 * 2.0 - 24.0)
                                                .max(0.0);
                                        ui.allocate_ui_with_layout(
                                            vec2(label_width, 50.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.set_min_size(vec2(label_width, 50.0));
                                                let mut text = text::LayoutJob::default();
                                                text.append(
                                                    &row.label,
                                                    0.0,
                                                    TextFormat::simple(
                                                        FontId::proportional(16.0),
                                                        if row.visible { TEXT } else { MUTED },
                                                    ),
                                                );
                                                if !row.note.is_empty() {
                                                    text.append(
                                                        &format!("\n{}", row.note),
                                                        0.0,
                                                        TextFormat::simple(
                                                            FontId::proportional(11.0),
                                                            MUTED,
                                                        ),
                                                    );
                                                }
                                                text.wrap.max_rows =
                                                    if row.note.is_empty() { 1 } else { 2 };
                                                ui.add(Label::new(text).wrap());
                                            },
                                        );
                                        let visibility = if row.locked {
                                            "Required"
                                        } else if row.visible {
                                            "Shown"
                                        } else {
                                            "Hidden"
                                        };
                                        let toggle = ui
                                            .add_enabled_ui(
                                                enabled
                                                    && !row.locked
                                                    && (!row.visible || row.can_hide),
                                                |ui| {
                                                    theme::button(
                                                        ui,
                                                        visibility,
                                                        vec2(100.0, 48.0),
                                                        row.visible,
                                                    )
                                                },
                                            )
                                            .inner;
                                        self.hit(
                                            &format!("Browser toggle {}", row.key),
                                            toggle.rect,
                                        );
                                        if toggle.clicked()
                                            && let Some(draft) = &mut self.browser_settings.draft
                                        {
                                            if let Some(id) = row.sort_id {
                                                draft.toggle_sort(id);
                                            } else {
                                                draft.toggle_category(&row.key);
                                            }
                                        }
                                        for (delta, arrow, active) in [
                                            (-1, "Up", row.visible && index > 0),
                                            (1, "Down", row.visible && index + 1 < shown),
                                        ] {
                                            let movement = ui
                                                .add_enabled_ui(enabled && active, |ui| {
                                                    theme::button(
                                                        ui,
                                                        arrow,
                                                        vec2(80.0, 48.0),
                                                        false,
                                                    )
                                                })
                                                .inner;
                                            self.hit(
                                                &format!("Browser move {} {delta}", row.key),
                                                movement.rect,
                                            );
                                            if movement.clicked()
                                                && let Some(draft) =
                                                    &mut self.browser_settings.draft
                                            {
                                                if let Some(id) = row.sort_id {
                                                    draft.move_sort(id, delta);
                                                } else {
                                                    draft.move_category(&row.key, delta);
                                                }
                                            }
                                        }
                                    });
                                });
                        });
                    }
                });
            });
        }
        let footer = Rect::from_min_max(pos2(area.left(), area.bottom() - footer_height), area.max);
        ui.scope_builder(UiBuilder::new().max_rect(footer), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            let writable = self.options.demo || (self.library_ready && self.fresh());
            let message = if !writable && !self.options.demo {
                "Reconnect before saving. Your draft stays here."
            } else {
                &self.browser_settings.message
            };
            ui.add(
                Label::new(RichText::new(message).size(13.0).color(
                    if self.browser_settings.reload {
                        ORANGE
                    } else {
                        MUTED
                    },
                ))
                .truncate(),
            )
            .on_hover_text(&self.browser_settings.message);
            let dirty = self.browser_settings.dirty();
            let pending = self.browser_settings.pending;
            ui.horizontal(|ui| {
                let save = ui
                    .add_enabled_ui(writable && enabled && dirty, |ui| {
                        theme::primary(ui, "Save browser", vec2(144.0, 48.0))
                    })
                    .inner;
                self.hit("Browser save", save.rect);
                if save.clicked() {
                    self.browser_settings_request(true);
                }
                let discard = ui
                    .add_enabled_ui(dirty && !pending, |ui| {
                        theme::button(ui, "Discard", vec2(112.0, 48.0), false)
                    })
                    .inner;
                self.hit("Browser discard", discard.rect);
                if discard.clicked() {
                    self.browser_settings.draft = self.browser_settings.snapshot.clone();
                }
                let reload = ui
                    .add_enabled_ui(!pending && (!dirty || self.browser_settings.reload), |ui| {
                        theme::button(ui, "Reload", vec2(112.0, 48.0), false)
                    })
                    .inner
                    .on_hover_text(
                        "Read current server values. After an error this replaces the draft.",
                    );
                self.hit("Browser reload", reload.rect);
                if reload.clicked() {
                    self.browser_settings_request(false);
                }
                ui.label(
                    RichText::new(if pending {
                        "Working…"
                    } else if dirty {
                        "Unsaved changes"
                    } else {
                        ""
                    })
                    .size(12.0)
                    .color(ORANGE),
                );
            });
        });
    }

    pub(super) fn browser_detail_picker(&mut self, ctx: &Context) {
        let Some(draft) = self.browser_settings.draft.as_ref() else {
            return;
        };
        let choices = draft.track_detail_fields.clone();
        let selected = draft.track_detail.clone();
        let response = Modal::new(Id::unique("browser-detail-picker")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(540.0));
            ui.heading("Track detail column");
            ui.label(
                RichText::new("Shown beside titles with DEFAULT sort on the CDJ.")
                    .size(13.0)
                    .color(MUTED),
            );
            ScrollArea::vertical()
                .id_salt("browser-detail-options")
                .scroll_source(scroll_area::ScrollSource::ALL)
                .max_height((ctx.content_rect().height() - 220.0).max(96.0))
                .min_scrolled_height((ctx.content_rect().height() - 220.0).max(96.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for field in choices {
                        let choice = theme::button(
                            ui,
                            &field.label,
                            vec2(ui.available_width(), 48.0),
                            field.key == selected,
                        );
                        self.hit(&format!("Browser detail {}", field.key), choice.rect);
                        if choice.clicked()
                            && let Some(draft) = &mut self.browser_settings.draft
                        {
                            draft.track_detail = field.key;
                            self.browser_settings.picker = false;
                        }
                    }
                });
            let cancel = theme::button(ui, "Cancel", vec2(ui.available_width(), 48.0), false);
            self.hit("Browser detail cancel", cancel.rect);
            if cancel.clicked() {
                self.browser_settings.picker = false;
            }
        });
        if response.should_close() {
            self.browser_settings.picker = false;
        }
    }
}

#[cfg(test)]
mod tests;
