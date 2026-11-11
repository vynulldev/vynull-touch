// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::device_settings::{Snapshot, choice_label, label};
use serde_json::Value;

#[derive(Default)]
pub(super) struct Editor {
    snapshot: Option<Snapshot>,
    draft: Value,
    pub pending: bool,
    reload: bool,
    serial: u64,
    tab: usize,
    pub picker: Option<(String, String)>,
    message: String,
}

impl Editor {
    fn dirty(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.full != self.draft)
    }
}

impl App {
    pub(super) fn open_device_settings(&mut self) {
        self.page = Page::DeviceSettings;
        self.keyboard = None;
        if self.device_settings.tab == 3 {
            self.open_browser_settings();
            return;
        }
        if !self.device_settings.pending && !self.device_settings.dirty() {
            self.device_settings_request(false);
        }
    }

    fn device_settings_request(&mut self, save: bool) {
        if self.device_settings.pending {
            return;
        }
        let changes = if save {
            if self.device_settings.reload {
                return;
            }
            let Some(snapshot) = &self.device_settings.snapshot else {
                return;
            };
            match snapshot.changes(&self.device_settings.draft) {
                Ok(changes) if !changes.is_empty() => Some(changes),
                Ok(_) => return,
                Err(error) => {
                    self.device_settings.message = error;
                    return;
                }
            }
        } else {
            None
        };
        self.device_settings.serial += 1;
        let token = self.device_settings.serial;
        self.device_settings.picker = None;
        if self.options.demo {
            let result = if let Some(changes) = &changes {
                let mut snapshot = self.device_settings.snapshot.clone().unwrap();
                snapshot.merge(changes).map(|full| {
                    snapshot.full = full;
                    snapshot
                })
            } else {
                Ok(self
                    .device_settings
                    .snapshot
                    .clone()
                    .unwrap_or_else(Snapshot::demo))
            };
            self.device_settings_response(token, save, result);
        } else if !self.library_ready {
            self.device_settings.message = "Connect to Vynull, then tap Reload.".into();
        } else {
            self.device_settings.pending = self.send(Command::DeviceSettings(token, changes));
            self.device_settings.message = if self.device_settings.pending {
                if save {
                    "Saving and reading back settings…"
                } else {
                    "Loading settings…"
                }
            } else {
                "Could not queue the request. Try again."
            }
            .into();
        }
    }

    pub(super) fn device_settings_response(
        &mut self,
        token: u64,
        written: bool,
        result: Result<Snapshot, String>,
    ) {
        let editor = &mut self.device_settings;
        if token != editor.serial {
            return;
        }
        editor.pending = false;
        match result {
            Ok(snapshot) => {
                editor.draft = snapshot.full.clone();
                editor.snapshot = Some(snapshot);
                editor.reload = false;
                editor.message = if self.options.demo {
                    "Demo profile · changes last for this demo session".into()
                } else if written {
                    "Saved and read back from Vynull · CDJ update requested".into()
                } else {
                    "Choose settings, then Save to update the shared profile.".into()
                };
            }
            Err(error) => {
                editor.reload = true;
                editor.message = error;
            }
        }
    }

    pub(super) fn device_settings_view(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let back = theme::button(ui, "‹ Settings", vec2(126.0, 48.0), false);
            self.hit("Device settings back", back.rect);
            if back.clicked() {
                self.page = Page::Settings;
            }
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.heading("CDJ & mixer");
                ui.label(
                    RichText::new("Shared hardware profile on Vynull")
                        .size(12.0)
                        .color(MUTED),
                )
                .on_hover_text(
                    "Available features and when settings take effect depend on your hardware.",
                );
            });
        });
        ui.horizontal(|ui| {
            for (i, name) in ["CDJ", "Display", "Mixer", "Browser"].iter().enumerate() {
                let changed = if i == 3 {
                    self.browser_settings.dirty()
                } else {
                    let sections: &[&str] = match i {
                        0 => &["my_setting"],
                        1 => &["my_setting_2", "dev_setting"],
                        _ => &["djm_my_setting"],
                    };
                    self.device_settings.snapshot.as_ref().is_some_and(|s| {
                        sections
                            .iter()
                            .any(|key| s.full[key] != self.device_settings.draft[key])
                    })
                };
                let title = format!("{name}{}", if changed { " *" } else { "" });
                let tab =
                    theme::button(ui, &title, vec2(112.0, 44.0), self.device_settings.tab == i);
                self.hit(&format!("Device tab {name}"), tab.rect);
                if tab.clicked() {
                    self.device_settings.tab = i;
                    self.browser_settings.picker = false;
                    if i == 3 {
                        self.open_browser_settings();
                    }
                }
            }
        });
        if self.device_settings.tab == 3 {
            self.browser_settings_view(ui);
            return;
        }
        let writable = self.options.demo || (self.library_ready && self.fresh());
        let pending = self.device_settings.pending;
        let dirty = self.device_settings.dirty();
        let message = if !writable && !self.options.demo {
            "Reconnect before saving. Your draft stays here.".to_owned()
        } else {
            self.device_settings.message.clone()
        };
        // Reserve the footer before laying out the scrollable settings rows.
        let area = ui.available_rect_before_wrap();
        let footer_height = 70.0;
        let body = Rect::from_min_max(
            area.min,
            pos2(
                area.right(),
                (area.bottom() - footer_height - 8.0).max(area.top()),
            ),
        );
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ScrollArea::vertical()
                .id_salt(("hardware-settings", self.device_settings.tab))
                .scroll_source(scroll_area::ScrollSource::ALL)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 6.0;
                    let sections: &[(&str, &str)] = match self.device_settings.tab {
                        1 => &[
                            ("dev_setting", "WAVEFORM & KEY"),
                            ("my_setting_2", "JOG DISPLAY & CONTROLS"),
                        ],
                        2 => &[("djm_my_setting", "MIXER")],
                        _ => &[("my_setting", "CDJ PLAYBACK & CONTROLS")],
                    };
                    for &(section, title) in sections {
                        if sections.len() > 1 {
                            ui.label(RichText::new(title).size(11.0).color(ORANGE));
                        }
                        let rows: Vec<_> = self.device_settings.draft[section]
                            .as_object()
                            .map(|fields| {
                                fields
                                    .iter()
                                    .filter_map(|(field, value)| {
                                        value
                                            .as_str()
                                            .map(|value| (field.clone(), value.to_owned()))
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if rows.is_empty() {
                            ui.label("Load settings from Vynull to view these options.");
                        }
                        for (field, value) in rows {
                            let known = self.device_settings.snapshot.as_ref().is_some_and(|s| {
                                s.enums
                                    .get(&field)
                                    .is_some_and(|choices| !choices.is_empty())
                            });
                            let changed = self
                                .device_settings
                                .snapshot
                                .as_ref()
                                .is_some_and(|s| s.full[section][&field] != value);
                            Frame::new()
                                .fill(PANEL)
                                .inner_margin(Margin::symmetric(10, 4))
                                .corner_radius(6)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let value_width =
                                            (ui.available_width() * 0.46).clamp(170.0, 300.0);
                                        let name_width =
                                            (ui.available_width() - value_width - 8.0).max(0.0);
                                        ui.allocate_ui_with_layout(
                                            vec2(name_width, 48.0),
                                            Layout::left_to_right(Align::Center),
                                            |ui| {
                                                ui.set_min_size(vec2(name_width, 48.0));
                                                ui.add(
                                                    Label::new(
                                                        RichText::new(format!(
                                                            "{}{}",
                                                            label(&field),
                                                            if changed { " *" } else { "" }
                                                        ))
                                                        .color(if changed { ORANGE } else { TEXT }),
                                                    )
                                                    .halign(Align::Min)
                                                    .truncate(),
                                                );
                                            },
                                        );
                                        let response = ui
                                            .add_enabled_ui(
                                                known && !pending && !self.device_settings.reload,
                                                |ui| {
                                                    theme::button(
                                                        ui,
                                                        &format!(
                                                            "{}  ›",
                                                            choice_label(&field, &value)
                                                        ),
                                                        vec2(value_width, 48.0),
                                                        changed,
                                                    )
                                                },
                                            )
                                            .inner;
                                        self.hit(
                                            &format!("Device {section}.{field}"),
                                            response.rect,
                                        );
                                        if response.clicked() {
                                            self.device_settings.picker =
                                                Some((section.into(), field.clone()));
                                        }
                                    });
                                });
                        }
                    }
                });
        });
        let footer = Rect::from_min_max(pos2(area.left(), area.bottom() - footer_height), area.max);
        ui.scope_builder(UiBuilder::new().max_rect(footer), |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.add(
                Label::new(RichText::new(message).size(13.0).color(
                    if self.device_settings.reload {
                        ORANGE
                    } else {
                        MUTED
                    },
                ))
                .truncate(),
            )
            .on_hover_text(&self.device_settings.message);
            ui.horizontal(|ui| {
                let save = ui
                    .add_enabled_ui(
                        writable && dirty && !pending && !self.device_settings.reload,
                        |ui| theme::primary(ui, "Save", vec2(112.0, 48.0)),
                    )
                    .inner;
                self.hit("Device save", save.rect);
                if save.clicked() {
                    self.device_settings_request(true);
                }
                let discard = ui
                    .add_enabled_ui(dirty && !pending, |ui| {
                        theme::button(ui, "Discard", vec2(112.0, 48.0), false)
                    })
                    .inner;
                self.hit("Device discard", discard.rect);
                if discard.clicked()
                    && let Some(snapshot) = &self.device_settings.snapshot
                {
                    self.device_settings.draft = snapshot.full.clone();
                    if !self.device_settings.reload {
                        self.device_settings.message = "Unsaved changes discarded.".into();
                    }
                }
                let reload = ui
                    .add_enabled_ui(!pending && (!dirty || self.device_settings.reload), |ui| {
                        theme::button(ui, "Reload", vec2(112.0, 48.0), false)
                    })
                    .inner
                    .on_hover_text(
                        "Read current server values. After an error this replaces the draft.",
                    );
                self.hit("Device reload", reload.rect);
                if reload.clicked() {
                    self.device_settings_request(false);
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

    pub(super) fn device_setting_picker(&mut self, ctx: &Context) {
        let Some((section, field)) = self.device_settings.picker.clone() else {
            return;
        };
        let choices = self
            .device_settings
            .snapshot
            .as_ref()
            .and_then(|s| s.enums.get(&field))
            .cloned()
            .unwrap_or_default();
        let selected = self.device_settings.draft[&section][&field]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let mut close = false;
        let response = Modal::new(Id::unique("device-setting-choice")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(540.0));
            ui.heading(label(&field));
            ui.label(
                RichText::new("Choose a value · Save applies your changes")
                    .size(13.0)
                    .color(MUTED),
            );
            ScrollArea::vertical()
                .id_salt(("device-choices", &section, &field))
                .scroll_source(scroll_area::ScrollSource::ALL)
                .max_height((ctx.content_rect().height() - 220.0).max(96.0))
                .show(ui, |ui| {
                    for value in &choices {
                        let choice = theme::button(
                            ui,
                            &choice_label(&field, value),
                            vec2(ui.available_width(), 48.0),
                            selected == *value,
                        );
                        self.hit(&format!("Device choice {value}"), choice.rect);
                        if choice.clicked() {
                            self.device_settings.draft[&section][&field] =
                                Value::String(value.clone());
                            close = true;
                        }
                    }
                });
            let cancel = theme::button(ui, "Cancel", vec2(ui.available_width(), 48.0), false);
            self.hit("Device choice cancel", cancel.rect);
            close |= cancel.clicked();
        });
        if close || response.should_close() {
            self.device_settings.picker = None;
        }
    }
}

#[cfg(test)]
mod tests;
