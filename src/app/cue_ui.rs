// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::cues::{self, Change, Editor};

#[cfg(test)]
mod tests;

const HOLD_SECONDS: f64 = 0.6;

pub(super) struct PadPress {
    number: u16,
    origin: Pos2,
    started: f64,
    cancelled: bool,
    held: bool,
}

impl App {
    pub(super) fn can_edit_cues(&self) -> bool {
        self.detail.as_ref().is_some_and(|d| d.cues_ready)
            && (self.options.demo || self.library_session.is_some())
            && self.cue_quick.is_none()
            && self.cue_editor.is_none()
    }

    pub(super) fn open_cue_editor(&mut self) {
        let Some(detail) = &self.detail else {
            return;
        };
        let slot = (1..=8)
            .find(|n| !detail.cues.iter().any(|c| c.number == *n))
            .unwrap_or(1);
        self.open_cue_slot(slot);
    }

    fn open_cue_slot(&mut self, slot: u16) {
        if !self.can_edit_cues() {
            return;
        }
        let (Some(track), Some(detail)) = (&self.detail_track, &self.detail) else {
            return;
        };
        self.preview.pause();
        self.cue_editor = Some(Editor::new(
            &detail.cues,
            slot,
            self.position,
            self.preview_duration(track),
            &detail.beat_grid,
        ));
    }

    fn tap_hot_cue(&mut self, slot: u16, duration: f64) {
        let Some(detail) = &self.detail else {
            return;
        };
        if let Some(cue) = detail.cues.iter().find(|c| c.number == slot) {
            if cue.kind == "loop" {
                match crate::loops::Region::from_cue(cue, duration) {
                    Ok(region) => {
                        self.position = region.start;
                        self.preview.start_loop(region);
                        self.loop_controls = true;
                        self.loop_error.clear();
                    }
                    Err(error) => self.loop_error = error,
                }
                return;
            }
            self.preview.change_loop(None, self.position);
            self.position = (f64::from(cue.time_ms) / 1000.0).min(duration);
            self.preview.seek(self.position, false);
            return;
        }
        if !self.can_edit_cues() {
            return;
        }
        // Quick creation always snaps, independently of the grid visibility and
        // the editor's Snap toggle. Do not silently create an unsnapped cue.
        if detail.beat_grid.is_empty() {
            self.notice = "No beat grid available; use Edit cues for free placement".into();
            return;
        }
        let mut editor = Editor::new(
            &detail.cues,
            slot,
            self.position,
            duration,
            &detail.beat_grid,
        );
        if !editor.error.is_empty() {
            self.notice = editor.error;
            return;
        }
        let cue = editor.draft.clone();
        self.cue_submit(&mut editor, Some(Change::Put(cue)));
        if editor.pending {
            self.cue_quick = Some(editor);
        } else if !editor.error.is_empty() {
            self.preview.pause();
            self.cue_editor = Some(editor);
        }
    }

    pub(super) fn hot_cue_pads(&mut self, ui: &mut Ui, duration: f64) {
        let cues = self
            .detail
            .as_ref()
            .map(|d| d.cues.clone())
            .unwrap_or_default();
        let can_edit = self.can_edit_cues() && duration > 0.0;
        let grid = self
            .detail
            .as_ref()
            .is_some_and(|d| !d.beat_grid.is_empty());
        let pending = self.cue_quick.as_ref().map(|e| e.draft.number);
        let (now, pressed, down, released, pointer) = ui.input(|i| {
            (
                i.time,
                i.pointer.primary_pressed(),
                i.pointer.primary_down(),
                i.pointer.primary_released(),
                i.pointer.interact_pos(),
            )
        });
        if pressed {
            self.cue_press = None;
        }
        ui.horizontal(|ui| {
            let width = (ui.available_width() - 7.0 * ui.spacing().item_spacing.x) / 8.0;
            for number in 1..=8 {
                let cue = cues.iter().find(|c| c.number == number);
                let saving = pending == Some(number);
                let enabled = (cue.is_some() || can_edit)
                    && self.cue_editor.is_none()
                    && !self.audio_dialog
                    && !self.load_dialog;
                let color = if saving {
                    ORANGE
                } else {
                    cue.map_or(MUTED, |c| cues::color(c.color_id))
                };
                let response = ui.add_enabled(
                    enabled,
                    Button::new("")
                        .min_size(vec2(width, 48.0))
                        .fill(if cue.is_some() || saving {
                            color.gamma_multiply(0.13)
                        } else {
                            CARD
                        })
                        .stroke(Stroke::new(
                            1.0,
                            if cue.is_some() || saving {
                                color.gamma_multiply(0.45)
                            } else {
                                LINE
                            },
                        ))
                        .corner_radius(8),
                );
                let pad = response.rect;
                self.hit(&format!("Cue {number}"), pad);
                let label = format!(
                    "{}{}",
                    cues::slot(number),
                    if cue.is_some_and(|c| c.kind == "loop") {
                        " ↻"
                    } else {
                        ""
                    }
                );
                let caption = if saving {
                    "Saving…".into()
                } else if let Some(cue) = cue {
                    time(f64::from(cue.time_ms) / 1000.0)
                } else if !self.detail.as_ref().is_some_and(|d| d.cues_ready) {
                    "Waiting…".into()
                } else if !grid {
                    "No grid".into()
                } else {
                    "Empty".into()
                };
                response.widget_info(|| {
                    WidgetInfo::labeled(
                        Role::Button,
                        enabled,
                        format!("Hot cue {label}: {caption}"),
                    )
                });
                let mut painter = ui.painter().clone();
                painter.multiply_opacity(if enabled || saving { 1.0 } else { 0.4 });
                painter.text(
                    pad.center_top() + vec2(0.0, 4.0),
                    Align2::CENTER_TOP,
                    label,
                    FontId::proportional(16.0),
                    color,
                );
                painter.text(
                    pad.center_bottom() - vec2(0.0, 5.0),
                    Align2::CENTER_BOTTOM,
                    caption,
                    FontId::proportional(11.0),
                    if cue.is_some() { TEXT } else { MUTED },
                );

                if pressed
                    && response.is_pointer_button_down_on()
                    && let Some(origin) = pointer
                {
                    self.cue_press = Some(PadPress {
                        number,
                        origin,
                        started: now,
                        cancelled: false,
                        held: false,
                    });
                }
                let mut hold = false;
                let mut suppress_tap = false;
                if let Some(press) = &mut self.cue_press
                    && press.number == number
                {
                    // Latch cancellation: dragging away and back must not create a cue.
                    press.cancelled |= !enabled
                        || pointer
                            .is_none_or(|p| p.distance(press.origin) > 6.0 || !pad.contains(p));
                    press.cancelled |= ui.input(|i| {
                        i.raw.events.iter().any(|event| match event {
                            eframe::egui::Event::PointerMoved(pos) => {
                                pos.distance(press.origin) > 6.0
                            }
                            eframe::egui::Event::PointerGone => !released,
                            eframe::egui::Event::WindowFocused(false) => true,
                            eframe::egui::Event::Touch {
                                phase: TouchPhase::Cancel,
                                ..
                            } => true,
                            _ => false,
                        })
                    });
                    if !press.cancelled && !press.held && (down || released) {
                        let elapsed = now - press.started;
                        if elapsed >= HOLD_SECONDS {
                            press.held = true;
                            hold = can_edit;
                        } else if down {
                            ui.ctx().request_repaint_after(Duration::from_millis(16));
                            let progress = Rect::from_min_size(
                                pad.left_bottom() - vec2(0.0, 3.0),
                                vec2(pad.width() * (elapsed / HOLD_SECONDS) as f32, 3.0),
                            );
                            painter.rect_filled(progress, 1, color);
                        }
                    }
                    suppress_tap = press.cancelled || press.held;
                }
                if hold {
                    self.open_cue_slot(number);
                } else if response.clicked() && !suppress_tap {
                    self.tap_hot_cue(number, duration);
                }
                response.on_hover_text(cue.map_or_else(
                    || "Tap to create at the nearest beat · hold to edit".into(),
                    |c| {
                        format!(
                            "{}{}Tap to jump · hold to edit",
                            c.label,
                            if c.label.is_empty() { "" } else { "\n" }
                        )
                    },
                ));
            }
        });
        if !down {
            self.cue_press = None;
        }
    }

    fn cue_submit(&mut self, editor: &mut Editor, change: Option<Change>) -> bool {
        let Some(track) = self.detail_track.clone() else {
            return false;
        };
        if editor.pending || self.cue_quick.is_some() {
            return false;
        }
        if let Some(Change::Put(cue)) = &change
            && let Err(e) = cues::validate(cue, self.preview_duration(&track))
        {
            editor.error = e;
            return false;
        }
        if self.options.demo {
            let Some(detail) = &mut self.detail else {
                return false;
            };
            if let Some(change) = change {
                detail.cues.retain(|c| c.number != change.number());
                detail.cues.extend(change.desired().cloned());
                detail.cues.sort_by_key(|c| c.number);
                self.demo_cues.insert(track.id, detail.cues.clone());
                self.demo_decks.remove(&track.id);
                self.notice = "Demo cue saved for this session".into();
                return true;
            }
            *editor = Editor::new(
                &detail.cues,
                editor.draft.number,
                self.position,
                f64::from(track.duration),
                &detail.beat_grid,
            );
        } else if self.send(Command::Cues {
            track: Box::new(track),
            token: self.token,
            expected: editor.expected.clone(),
            change,
        }) {
            editor.pending = true;
            editor.error.clear();
        } else {
            editor.error = "Cue edit was not sent; request queue is busy".into();
        }
        false
    }

    pub(super) fn cue_modal(&mut self, ctx: &Context) {
        let Some(mut editor) = self.cue_editor.take() else {
            return;
        };
        let (Some(track), Some(detail)) = (self.detail_track.clone(), self.detail.clone()) else {
            return;
        };
        let duration = self.preview_duration(&track);
        let mut close = false;
        let was_typing = editor.typing;
        let modal = Modal::new(Id::unique("cue-editor")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(if editor.typing {
                700.0
            } else {
                650.0
            }));
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.heading(if editor.typing {
                "Cue label".into()
            } else {
                format!(
                    "{} {}",
                    if editor.draft.kind == "loop" {
                        "Loop"
                    } else {
                        "Cue"
                    },
                    cues::slot(editor.draft.number)
                )
            });
            if editor.typing {
                let response = ui.add_sized(
                    vec2(ui.available_width(), 48.0),
                    TextEdit::singleline(&mut editor.draft.label).char_limit(128),
                );
                self.hit("Cue label", response.rect);
                self.cue_label_keyboard(ui, &mut editor);
                return;
            }
            ui.add_enabled_ui(!editor.pending, |ui| {
                ScrollArea::horizontal()
                    .id_salt("cue-slots")
                    .scroll_source(scroll_area::ScrollSource::ALL)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut slots: Vec<u16> = (1..=8)
                                .chain(detail.cues.iter().map(|c| c.number))
                                .collect();
                            slots.sort_unstable();
                            slots.dedup();
                            for number in slots {
                                let response = theme::button(
                                    ui,
                                    &cues::slot(number),
                                    vec2(48.0, 48.0),
                                    editor.draft.number == number,
                                );
                                self.hit(&format!("Edit slot {number}"), response.rect);
                                if response.clicked() {
                                    editor = editor.select(
                                        &detail.cues,
                                        number,
                                        self.position,
                                        duration,
                                        &detail.beat_grid,
                                    );
                                }
                            }
                            if theme::button(ui, "+ Memory", vec2(104.0, 48.0), false).clicked()
                                && let Some(number) = (9..=u16::MAX)
                                    .find(|n| !detail.cues.iter().any(|c| c.number == *n))
                            {
                                editor = editor.select(
                                    &detail.cues,
                                    number,
                                    self.position,
                                    duration,
                                    &detail.beat_grid,
                                );
                            }
                        });
                    });
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(cues::timestamp(editor.draft.time_ms))
                            .monospace()
                            .size(22.0),
                    );
                    let place = theme::button(ui, "At playhead", vec2(130.0, 48.0), false);
                    self.hit("Cue at playhead", place.rect);
                    if place.clicked() {
                        editor.place(self.position, duration, &detail.beat_grid);
                    }
                    let snap = ui
                        .add_enabled_ui(!detail.beat_grid.is_empty(), |ui| {
                            theme::button(ui, "Snap", vec2(76.0, 48.0), editor.snap)
                        })
                        .inner;
                    self.hit("Cue snap", snap.rect);
                    if snap.clicked() {
                        editor.snap = !editor.snap;
                    }
                    for (label, delta) in [("−", -0.01), ("+", 0.01)] {
                        let response = theme::button(ui, label, vec2(48.0, 48.0), false);
                        self.hit(&format!("Cue nudge {label}"), response.rect);
                        if response.clicked() {
                            // Fine nudges explicitly choose free placement.
                            editor.snap = false;
                            editor.place(
                                (f64::from(editor.draft.time_ms) / 1000.0 + delta)
                                    .clamp(0.0, duration),
                                duration,
                                &detail.beat_grid,
                            );
                        }
                    }
                });
                ui.label(
                    RichText::new(if editor.draft.kind == "loop" {
                        format!(
                            "Out {} · moving In keeps the loop length",
                            cues::timestamp(editor.draft.loop_ms.max(0) as u32)
                        )
                    } else if editor.snap {
                        "Placement snaps to the saved beat grid · +/− nudges 10 ms".into()
                    } else {
                        "Free placement · +/− nudges 10 ms".into()
                    })
                    .size(13.0)
                    .color(MUTED),
                );
                let label = ui.add_sized(
                    vec2(ui.available_width(), 48.0),
                    TextEdit::singleline(&mut editor.draft.label)
                        .hint_text("Cue label…")
                        .char_limit(128),
                );
                self.hit("Cue label", label.rect);
                if label.clicked() {
                    editor.typing = true;
                }
                ui.horizontal(|ui| {
                    for (id, name) in cues::PALETTE {
                        let response = ui.add_sized(
                            vec2(48.0, 48.0),
                            Button::new(if id == 0 { "D" } else { "" })
                                .fill(cues::color(id).gamma_multiply(0.7))
                                .stroke(Stroke::new(
                                    if editor.draft.color_id == id {
                                        2.0
                                    } else {
                                        0.0
                                    },
                                    TEXT,
                                )),
                        );
                        self.hit(&format!("Cue color {id}"), response.rect);
                        if response.clicked() {
                            editor.draft.color_id = id;
                        }
                        response.on_hover_text(name);
                    }
                });
                ui.horizontal(|ui| {
                    let cancel = theme::button(ui, "Cancel", vec2(100.0, 48.0), false);
                    self.hit("Cue cancel", cancel.rect);
                    if cancel.clicked() {
                        close = true;
                    }
                    let reload = theme::button(ui, "Reload", vec2(100.0, 48.0), false);
                    self.hit("Cue reload", reload.rect);
                    if reload.clicked() {
                        self.cue_submit(&mut editor, None);
                    }
                    let delete = ui
                        .add_enabled_ui(editor.expected.is_some(), |ui| {
                            theme::button(ui, "Delete", vec2(100.0, 48.0), false)
                        })
                        .inner;
                    self.hit("Cue delete", delete.rect);
                    if delete.clicked() {
                        let number = editor.draft.number;
                        close = self.cue_submit(&mut editor, Some(Change::Delete(number)));
                    }
                    let save = theme::primary(ui, "Save", vec2(110.0, 48.0));
                    self.hit("Cue save", save.rect);
                    if save.clicked() {
                        let cue = editor.draft.clone();
                        close = self.cue_submit(&mut editor, Some(Change::Put(cue)));
                    }
                });
            });
            if editor.pending {
                ui.label("Saving and verifying cues…");
            }
            if !editor.error.is_empty() {
                ui.add(Label::new(RichText::new(&editor.error).size(13.0).color(ORANGE)).wrap());
            }
        });
        if modal.should_close() && !editor.pending {
            if was_typing {
                editor.typing = false;
            } else {
                close = true;
            }
        }
        if !close {
            self.cue_editor = Some(editor);
        }
    }

    fn cue_label_keyboard(&mut self, ui: &mut Ui, editor: &mut Editor) {
        let key_width = (ui.available_width() - 128.0 - 50.0) / 10.0;
        ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
        let rows = if self.symbols {
            ["1234567890", "-/:.=_@?&", "+#%~[]()"]
        } else {
            ["qwertyuiop", "asdfghjkl", "zxcvbnm"]
        };
        for (row_index, row) in rows.into_iter().enumerate() {
            ui.horizontal(|ui| {
                if row_index > 0 {
                    ui.add_space(
                        (ui.available_width() - row.len() as f32 * (key_width + 5.0) + 5.0) / 2.0,
                    );
                }
                for key in row.chars() {
                    let response =
                        theme::button(ui, &key.to_string(), vec2(key_width, 48.0), false);
                    self.hit(&format!("Cue key {key}"), response.rect);
                    if response.clicked() && editor.draft.label.chars().count() < 128 {
                        editor.draft.label.push(key);
                    }
                }
                if row_index == 0 {
                    let backspace = theme::button(ui, "Backspace", vec2(128.0, 48.0), false);
                    self.hit("Cue backspace", backspace.rect);
                    if backspace.clicked() {
                        editor.draft.label.pop();
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if theme::button(
                ui,
                if self.symbols { "ABC" } else { "123" },
                vec2(70.0, 48.0),
                false,
            )
            .clicked()
            {
                self.symbols = !self.symbols;
            }
            if theme::button(ui, "Clear", vec2(82.0, 48.0), false).clicked() {
                editor.draft.label.clear();
            }
            let done_width = 100.0;
            let space_width =
                (ui.available_width() - done_width - ui.spacing().item_spacing.x).max(0.0);
            if theme::button(ui, "Space", vec2(space_width, 48.0), false).clicked()
                && editor.draft.label.chars().count() < 128
            {
                editor.draft.label.push(' ');
            }
            let done = theme::primary(ui, "Done", vec2(done_width, 48.0));
            self.hit("Cue label done", done.rect);
            if done.clicked() {
                editor.typing = false;
            }
        });
    }
}
