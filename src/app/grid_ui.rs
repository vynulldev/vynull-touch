// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::grid_edit::{Change, Settings, Snapshot};

#[cfg(test)]
mod tests;

pub(super) struct Editor {
    pub open: bool,
    pub pending: bool,
    pub needs_reload: bool,
    snapshot: Option<Snapshot>,
    bpm: String,
    typing: bool,
    step: f64,
    status: String,
}

impl Editor {
    fn new() -> Self {
        Self {
            open: true,
            pending: false,
            needs_reload: true,
            snapshot: None,
            bpm: String::new(),
            typing: false,
            step: 1.0,
            status: String::new(),
        }
    }
    fn writable(&self) -> bool {
        !self.pending
            && !self.needs_reload
            && self
                .snapshot
                .as_ref()
                .is_some_and(|s| !s.track.analysis_locked && !s.grid.is_empty())
    }
    fn tempo(&self) -> Option<f64> {
        self.bpm
            .parse::<f64>()
            .ok()
            .filter(|n| n.is_finite() && (20.0..=400.0).contains(n))
    }
}

impl App {
    pub(super) fn open_grid_editor(&mut self) {
        if self.detail_busy || self.detail.is_none() || self.cue_quick.is_some() {
            return;
        }
        self.preview.pause();
        self.wave_drag_start = None;
        self.cue_press = None;
        let mut editor = self.grid_editor.take().unwrap_or_else(Editor::new);
        editor.open = true;
        self.grid_submit(&mut editor, None);
        self.grid_editor = Some(editor);
    }

    fn grid_submit(&mut self, editor: &mut Editor, change: Option<Change>) {
        if editor.pending || (change.is_some() && !editor.writable()) {
            return;
        }
        let Some(track) = self.detail_track.clone() else {
            return;
        };
        if let Some(change) = change {
            if self.preview.request.dragging {
                return;
            }
            if let Err(error) = change.apply(editor.snapshot.as_ref().unwrap()) {
                editor.status = error.into();
                return;
            }
        }
        editor.status.clear();
        if self.options.demo {
            let before = self
                .demo_grids
                .get(&track.id)
                .cloned()
                .unwrap_or_else(|| Snapshot {
                    grid: self.detail.as_ref().unwrap().beat_grid.clone(),
                    bpm: track.bpm,
                    duration_sec: track.duration,
                    settings: Settings {
                        bpm: track.bpm,
                        beat_phase_shift: 0,
                    },
                    track,
                });
            let result = change.map_or_else(
                || Ok(before.clone()),
                |change| change.apply(&before).map_err(str::to_owned),
            );
            self.grid_complete(editor, change.is_some(), result);
        } else {
            editor.pending = self.send(Command::Grid {
                track: Box::new(track),
                token: self.token,
                expected: editor.snapshot.clone().map(Box::new),
                change,
            });
            if !editor.pending {
                editor.status = self.notice.clone();
            }
        }
    }

    pub(super) fn grid_reply(&mut self, edited: bool, result: Result<Box<Snapshot>, String>) {
        if let Some(mut editor) = self.grid_editor.take() {
            self.grid_complete(&mut editor, edited, result.map(|s| *s));
            self.grid_editor = Some(editor);
        }
    }

    fn grid_complete(
        &mut self,
        editor: &mut Editor,
        edited: bool,
        result: Result<Snapshot, String>,
    ) {
        editor.pending = false;
        if let Some(track) = &self.detail_track {
            self.demo_decks.remove(&track.id);
            if let Some(cache) = &mut self.deck_cache {
                cache.invalidate(track.id);
            }
            self.deck_clocks
                .retain(|key, _| key.external || key.track != track.id);
        }
        match result {
            Err(error) => {
                editor.needs_reload = true;
                // The daemon may have changed while its reply was lost. Avoid
                // snapping new cues or loops to a grid we can no longer confirm.
                if let Some(detail) = &mut self.detail {
                    detail.beat_grid = Default::default();
                }
                editor.status = format!("Reload required · {error}");
                self.notice = editor.status.clone();
            }
            Ok(saved) => {
                if !self
                    .detail_track
                    .as_ref()
                    .is_some_and(|t| t.id == saved.track.id && t.file_path == saved.track.file_path)
                {
                    return;
                }
                editor.needs_reload = false;
                editor.bpm = format!("{:.2}", saved.bpm);
                editor.status = if saved.track.analysis_locked {
                    "Analysis is locked. Unlock it in Vynull before editing.".into()
                } else if saved.grid.is_empty() {
                    "No beat grid. Analyze this track in Vynull first.".into()
                } else if !edited {
                    "Nudges preserve timing · Apply BPM rebuilds a steady grid".into()
                } else if self.options.demo {
                    "Applied · demo session only".into()
                } else {
                    "Applied and read back from Vynull".into()
                };
                if let Some(detail) = &mut self.detail {
                    detail.beat_grid = saved.grid.clone();
                }
                self.detail_track = Some(saved.track.clone());
                for track in self.all_tracks.iter_mut().chain(&mut self.tracks) {
                    if track.id == saved.track.id && track.file_path == saved.track.file_path {
                        *track = saved.track.clone();
                    }
                }
                if self.options.demo {
                    self.demo_grids.insert(saved.track.id, saved.clone());
                }
                self.reindex();
                self.show_grid = true;
                self.notice = editor.status.clone();
                editor.snapshot = Some(saved);
            }
        }
    }

    fn grid_button(&mut self, ui: &mut Ui, label: &str, width: f32, active: bool) -> bool {
        let button = theme::button(ui, label, vec2(width, 48.0), active);
        self.hit(&format!("Grid {label}"), button.rect);
        button.clicked()
    }

    pub(super) fn grid_editing(&self) -> bool {
        self.page == Page::Detail && self.grid_editor.as_ref().is_some_and(|e| e.open)
    }

    pub(super) fn grid_pending(&self) -> bool {
        self.grid_editor.as_ref().is_some_and(|e| e.pending)
    }

    pub(super) fn finish_grid_editing(&mut self) {
        if let Some(editor) = &mut self.grid_editor
            && !editor.pending
        {
            editor.open = false;
            editor.typing = false;
            self.wave_drag_start = None;
        }
    }

    pub(super) fn grid_status(&self) -> String {
        let Some(editor) = &self.grid_editor else {
            return String::new();
        };
        if editor.pending {
            "Waiting for Vynull…".into()
        } else if !self.options.demo && self.library_session.is_none() {
            "Reconnect to Vynull with library identity support before editing.".into()
        } else if editor.tempo().is_none() && editor.writable() {
            "Enter a BPM between 20 and 400.".into()
        } else {
            editor.status.clone()
        }
    }

    // These two rows replace hot cues and beat-jump/loop controls. They use the
    // same space, leaving both Inspect waveforms and the fixed footer in place.
    pub(super) fn grid_controls(&mut self, ui: &mut Ui) {
        let Some(mut editor) = self.grid_editor.take() else {
            return;
        };
        let mut action = None;
        let writable = editor.writable() && (self.options.demo || self.library_session.is_some());
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_enabled_ui(writable, |ui| {
                ui.horizontal(|ui| {
                    let direction_width = ((ui.available_width() - 56.0 - 4.0 * 84.0 - 6.0 * 6.0) / 2.0).max(104.0);
                    ui.add_sized(vec2(56.0, 48.0), Label::new("Nudge"));
                    for ms in [0.1, 1.0, 10.0, 50.0] {
                        if self.grid_button(ui, &format!("{ms} ms"), 84.0, editor.step == ms) {
                            editor.step = ms;
                        }
                    }
                    if self.grid_button(ui, "‹ Earlier", direction_width, false) {
                        action = Some(Change::Nudge(-editor.step));
                    }
                    if self.grid_button(ui, "Later ›", direction_width, false) {
                        action = Some(Change::Nudge(editor.step));
                    }
                });
                ui.horizontal(|ui| {
                    let field_width = (ui.available_width() - 56.0 - 4.0 * 84.0 - 128.0 - 6.0 * 6.0).clamp(100.0, 200.0);
                    ui.add_sized(vec2(56.0, 48.0), Label::new("BPM"));
                    let field = theme::button(ui, &editor.bpm, vec2(field_width, 48.0), false);
                    self.hit("Grid BPM", field.rect);
                    if field.clicked() {
                        editor.typing = true;
                    }
                    for (label, factor, offset) in [
                        ("−0.01", 1.0, -0.01),
                        ("+0.01", 1.0, 0.01),
                        ("÷2", 0.5, 0.0),
                        ("×2", 2.0, 0.0),
                    ] {
                        if self.grid_button(ui, label, 84.0, false)
                            && let Some(bpm) = editor.tempo()
                        {
                            editor.bpm = format!("{:.2}", (bpm * factor + offset).clamp(20.0, 400.0));
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(editor.tempo().is_some(), |ui| {
                            if self.grid_button(ui, "Apply BPM", 128.0, false) {
                                action = editor.tempo().map(Change::Tempo);
                            }
                        }).response.on_hover_text("Save BPM and rebuild a steady grid from the current origin. Variable beat timing will be replaced.");
                    });
                });
            });
        });
        if let Some(change) = action {
            self.grid_submit(&mut editor, Some(change));
        }
        self.grid_editor = Some(editor);
    }

    pub(super) fn grid_actions(&mut self, ui: &mut Ui, track: &Track) {
        let Some(mut editor) = self.grid_editor.take() else {
            return;
        };
        let mut reload = false;
        let mut align = false;
        let mut close = false;
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.horizontal(|ui| {
            ui.add_enabled_ui(!editor.pending, |ui| {
                let playing = self.preview.request.playing
                    && !matches!(self.preview.state.phase, Phase::Error | Phase::Ended);
                ui.add_enabled_ui(
                    self.options.demo || (!track.file_missing && self.library_session.is_some()),
                    |ui| {
                        if self.grid_button(
                            ui,
                            if playing { "Pause" } else { "Play" },
                            84.0,
                            playing,
                        ) {
                            self.preview.toggle(self.position);
                            if self.preview.state.phase == Phase::Ended {
                                self.position = 0.0;
                            }
                        }
                    },
                );
                let writable =
                    editor.writable() && (self.options.demo || self.library_session.is_some());
                ui.add_enabled_ui(
                    writable && self.position < self.preview_duration(track),
                    |ui| {
                        align = self.grid_button(ui, "Set beat 1 here", 176.0, false);
                    },
                );
                reload = self.grid_button(ui, "Reload grid", 112.0, false);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let done = theme::primary(ui, "Done editing", vec2(136.0, 48.0));
                    self.hit("Grid Done editing", done.rect);
                    close = done.clicked();
                    if self.grid_button(ui, "+", 48.0, false) {
                        self.set_zoom((self.zoom * 2.0).min(256.0));
                    }
                    if self.grid_button(ui, "−", 48.0, false) {
                        self.set_zoom((self.zoom / 2.0).max(1.0));
                    }
                });
            });
        });
        if reload {
            self.grid_submit(&mut editor, None);
        } else if align {
            self.grid_submit(&mut editor, Some(Change::Downbeat(self.position * 1000.0)));
        }
        self.grid_editor = Some(editor);
        if close {
            self.finish_grid_editing();
        }
    }

    // Only numeric entry needs a popup; the editor itself lives in Inspect.
    pub(super) fn grid_keyboard(&mut self, ctx: &Context) {
        let Some(mut editor) = self.grid_editor.take() else {
            return;
        };
        let was_typing = editor.typing;
        if editor.typing {
            Modal::new(Id::unique("grid-bpm-keypad")).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 64.0).min(640.0));
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.heading("Grid BPM");
                let field = ui.add_sized(
                    vec2(ui.available_width(), 48.0),
                    TextEdit::singleline(&mut editor.bpm).char_limit(8),
                );
                self.hit("Grid BPM entry", field.rect);
                let width = (ui.available_width() - 3.0 * 6.0) / 4.0;
                for row in [
                    ["7", "8", "9", "Backspace"],
                    ["4", "5", "6", "Clear"],
                    ["1", "2", "3", "Cancel"],
                    ["0", ".", "", "Done"],
                ] {
                    ui.horizontal(|ui| {
                        for key in row {
                            if key.is_empty() {
                                ui.allocate_space(vec2(width, 48.0));
                                continue;
                            }
                            if self.grid_button(ui, key, width, false) {
                                match key {
                                    "Backspace" => {
                                        editor.bpm.pop();
                                    }
                                    "Clear" => editor.bpm.clear(),
                                    "Cancel" => {
                                        editor.bpm = editor
                                            .snapshot
                                            .as_ref()
                                            .map_or(String::new(), |s| format!("{:.2}", s.bpm));
                                        editor.typing = false;
                                    }
                                    "Done" => editor.typing = false,
                                    _ if editor.bpm.len() < 8 => editor.bpm.push_str(key),
                                    _ => (),
                                }
                            }
                        }
                    });
                }
                ui.label("20–400 BPM · Done returns to the editor; Apply BPM saves.");
            });
        }
        let escape = ctx.input(|i| i.key_pressed(Key::Escape)) && !editor.pending;
        if escape && was_typing {
            editor.typing = false;
        }
        self.grid_editor = Some(editor);
        if escape && !was_typing {
            self.finish_grid_editing();
        }
    }
}
