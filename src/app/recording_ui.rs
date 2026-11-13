// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::recording;
use std::sync::mpsc;
const RED: Color32 = Color32::from_rgb(255, 92, 100);

pub(super) struct Recorder {
    pub controller: recording::Controller,
    pub preferences: recording::Preferences,
    devices: Vec<recording::Device>,
    scan: Option<mpsc::Receiver<Result<Vec<recording::Device>, String>>>,
    picker: bool,
    format_picker: bool,
    error: String,
    confirm_close: bool,
    closing: bool,
    allow_close: bool,
}
impl Recorder {
    pub fn new(ctx: Context, preferences: recording::Preferences) -> Self {
        Self {
            controller: recording::Controller::new(ctx),
            preferences,
            devices: vec![],
            scan: None,
            picker: false,
            format_picker: false,
            error: String::new(),
            confirm_close: false,
            closing: false,
            allow_close: false,
        }
    }
}

impl App {
    pub(super) fn poll_recorder(&mut self, ctx: &Context) {
        self.recorder.controller.poll();
        if let Some(result) = self.recorder.scan.as_ref().and_then(|r| r.try_recv().ok()) {
            self.recorder.scan = None;
            match result {
                Ok(devices) => self.recorder.devices = devices,
                Err(e) => self.recorder.error = e,
            }
        }
        if self.recorder.closing && !self.recorder.controller.busy() {
            self.recorder.closing = false;
            if self.recorder.controller.state.error.is_some() {
                self.recorder.confirm_close = true;
            } else {
                self.recorder.allow_close = true;
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.recorder.allow_close
            && (self.recorder.controller.state.recording || self.recorder.controller.busy())
        {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.recorder.confirm_close = true;
        }
    }

    pub(super) fn save_recording_preferences(&mut self) {
        if let Err(e) = self.recorder.preferences.directory() {
            self.recorder.error = e;
            return;
        }
        self.recorder.error.clear();
        if let Some(path) = &self.settings_path
            && let Err(e) = settings::save_recording(path, self.recorder.preferences.clone())
        {
            self.recorder.error = format!("Could not save recording settings: {e}");
        }
    }

    pub(super) fn recording_status(&self) -> Option<(String, Color32)> {
        let state = &self.recorder.controller.state;
        if state.recording {
            Some((format!("REC {}", state.elapsed()), RED))
        } else if state.error.is_some() {
            Some(("Recording error · see Record".into(), RED))
        } else {
            None
        }
    }

    pub(super) fn recording_view(&mut self, ui: &mut Ui) {
        let state = self.recorder.controller.state.clone();
        let busy = self.recorder.controller.busy();
        ScrollArea::vertical()
            .id_salt("session-recorder")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Session recording").size(23.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(state.elapsed())
                                .monospace()
                                .size(27.0)
                                .color(if state.recording { RED } else { TEXT }),
                        );
                    });
                });
                ui.label(
                    RichText::new("Record the audio input on this device.")
                        .size(13.0)
                        .color(MUTED),
                );
                ui.add_space(4.0);
                ui.add_enabled_ui(!state.active && !busy, |ui| {
                    ui.horizontal(|ui| {
                        let name = self
                            .recorder
                            .preferences
                            .input
                            .as_ref()
                            .map(|d| format!("{} · {}", d.name, d.host))
                            .unwrap_or("System default input".into());
                        let input = ui.add_sized(
                            [
                                ui.available_width() - 200.0 - ui.spacing().item_spacing.x,
                                48.0,
                            ],
                            Button::new(RichText::new(format!("Input: {name}")).size(16.0))
                                .truncate(),
                        );
                        self.hit("Recording input", input.rect);
                        if input.clicked() {
                            self.recorder.picker = true;
                            self.scan_recording_inputs(ui.ctx());
                        }
                        let format = theme::button(
                            ui,
                            &format!(
                                "{} · {}",
                                self.recorder.preferences.sample_rate.label(),
                                self.recorder.preferences.bit_depth.label()
                            ),
                            vec2(200.0, 48.0),
                            false,
                        );
                        self.hit("Recording format", format.rect);
                        if format.clicked() {
                            self.recorder.format_picker = true;
                        }
                    });
                })
                .response
                .on_hover_text(if state.active {
                    "Close the input before changing the input or recording format."
                } else {
                    "Choose the input and recording format."
                });
                ui.add_enabled_ui(!state.recording && !busy, |ui| {
                    if self.keyboard == Some(Field::RecordingDirectory) {
                        let response = ui.add_sized(
                            [ui.available_width(), 48.0],
                            TextEdit::singleline(&mut self.recorder.preferences.directory)
                                .font(FontId::proportional(17.0)),
                        );
                        self.hit("Recording folder edit", response.rect);
                    } else {
                        let folder = ui.add_sized(
                            [ui.available_width(), 48.0],
                            Button::new(
                                RichText::new(format!(
                                    "Save to: {}",
                                    self.recorder.preferences.directory
                                ))
                                .size(15.0),
                            )
                            .truncate(),
                        );
                        self.hit("Recording folder", folder.rect);
                        if folder.clicked() {
                            self.keyboard = Some(Field::RecordingDirectory);
                        }
                    }
                });
                let response = meters(ui, &state);
                self.hit("Recording meters", response.rect);
                if response.clicked() && state.active && !busy {
                    self.recorder.controller.clear_clips();
                }
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                    ui.add_enabled_ui(!busy && self.keyboard.is_none(), |ui| {
                        let label = if busy {
                            "Please wait…"
                        } else if state.recording {
                            "Stop & save"
                        } else {
                            "Start recording"
                        };
                        let record = theme::primary(ui, label, vec2(width, 52.0));
                        self.hit("Recording start stop", record.rect);
                        if record.clicked() {
                            if state.recording {
                                self.recorder.controller.stop();
                            } else {
                                self.save_recording_preferences();
                                match self.recorder.preferences.directory() {
                                    Ok(directory) => {
                                        if state.active {
                                            self.recorder.controller.record(directory);
                                        } else {
                                            self.recorder
                                                .controller
                                                .begin(self.recorder.preferences.clone(), true);
                                        }
                                    }
                                    Err(e) => self.recorder.error = e,
                                }
                            }
                        }
                    });
                    ui.add_enabled_ui(!busy && !state.recording && self.keyboard.is_none(), |ui| {
                        let monitor = theme::button(
                            ui,
                            if state.active {
                                "Close input"
                            } else {
                                "Check levels"
                            },
                            vec2(width, 52.0),
                            state.active,
                        );
                        self.hit("Recording monitor", monitor.rect);
                        if monitor.clicked() {
                            if state.active {
                                self.recorder.controller.stop();
                            } else {
                                self.recorder
                                    .controller
                                    .begin(self.recorder.preferences.clone(), false);
                            }
                        }
                    });
                });
                let info = if state.active {
                    format!(
                        "{} · {:.1} kHz · stereo{}",
                        state.input_name,
                        state.rate as f32 / 1000.0,
                        if state.recording {
                            format!(" · {} WAV", state.bit_depth.label())
                        } else {
                            " · meters only".into()
                        }
                    )
                } else {
                    "Stereo WAV · numbered 1 GB parts for long sessions".into()
                };
                ui.add(Label::new(RichText::new(info).size(12.0).color(MUTED)).truncate());
                if let Some(path) = state.paths.last() {
                    let prefix = if state.recording {
                        "Writing"
                    } else if state.error.is_some() {
                        "Partial recording"
                    } else {
                        "Saved"
                    };
                    let text = format!(
                        "{prefix}: {}{}",
                        recording::display_path(path),
                        if state.paths.len() > 1 {
                            format!(" ({} parts)", state.paths.len())
                        } else {
                            String::new()
                        }
                    );
                    ui.add(
                        Label::new(
                            RichText::new(&text)
                                .size(13.0)
                                .color(if state.error.is_some() { ORANGE } else { GREEN }),
                        )
                        .truncate(),
                    )
                    .on_hover_text(text);
                }
                if let Some(error) = &state.error {
                    ui.colored_label(RED, error);
                }
                if !self.recorder.error.is_empty() {
                    ui.colored_label(RED, &self.recorder.error);
                }
            });
    }

    fn scan_recording_inputs(&mut self, ctx: &Context) {
        if self.recorder.scan.is_some() {
            return;
        }
        let (tx, rx) = mpsc::sync_channel(1);
        self.recorder.scan = Some(rx);
        self.recorder.error.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(recording::devices());
            ctx.request_repaint();
        });
    }

    pub(super) fn recording_modals(&mut self, ctx: &Context) {
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.recorder.picker = false;
            self.recorder.format_picker = false;
            self.recorder.confirm_close = false;
        }
        if self.recorder.picker {
            Modal::new(Id::unique("recording-input-picker")).show(ctx, |ui| {
                ui.set_width((ctx.content_rect().width() - 64.0).min(600.0));
                ui.heading("Recording input");
                ui.label("Choose the stereo input connected to your mixer.");
                ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 220.0).max(100.0))
                    .show(ui, |ui| {
                        let mut choices = vec![(None, "System default input".to_string())];
                        choices.extend(
                            self.recorder
                                .devices
                                .iter()
                                .map(|d| (Some(d.clone()), format!("{} · {}", d.name, d.host))),
                        );
                        for (device, label) in choices {
                            if ui
                                .add_sized(
                                    [ui.available_width(), 48.0],
                                    Button::new(label)
                                        .selected(device == self.recorder.preferences.input)
                                        .truncate(),
                                )
                                .clicked()
                            {
                                self.recorder.preferences.input = device;
                                self.save_recording_preferences();
                                self.recorder.picker = false;
                            }
                        }
                    });
                if self.recorder.scan.is_some() {
                    ui.label("Finding audio inputs…");
                }
                if !self.recorder.error.is_empty() {
                    ui.colored_label(RED, &self.recorder.error);
                }
                ui.horizontal(|ui| {
                    if theme::button(ui, "Refresh", vec2(120.0, 48.0), false).clicked() {
                        self.scan_recording_inputs(ctx);
                    }
                    if theme::button(ui, "Close", vec2(120.0, 48.0), false).clicked() {
                        self.recorder.picker = false;
                    }
                });
            });
        }
        if self.recorder.format_picker {
            self.recording_format_modal(ctx);
        }
        if self.recorder.confirm_close {
            Modal::new(Id::unique("recording-close")).show(ctx, |ui| {
                ui.set_width(440.0_f32.min(ctx.content_rect().width() - 48.0));
                ui.heading("Finish recording before closing?");
                ui.label("The current WAV will be saved before the app exits.");
                if let Some(e) = &self.recorder.controller.state.error {
                    ui.colored_label(RED, e);
                }
                ui.add_enabled_ui(!self.recorder.controller.busy(), |ui| {
                    ui.horizontal(|ui| {
                        if theme::button(ui, "Keep open", vec2(150.0, 48.0), false).clicked() {
                            self.recorder.confirm_close = false;
                        }
                        if theme::primary(
                            ui,
                            if self.recorder.controller.state.active {
                                "Stop & quit"
                            } else {
                                "Quit"
                            },
                            vec2(170.0, 48.0),
                        )
                        .clicked()
                        {
                            self.recorder.confirm_close = false;
                            if self.recorder.controller.state.active {
                                self.recorder.controller.stop();
                                self.recorder.closing = true;
                            } else {
                                self.recorder.allow_close = true;
                                ctx.send_viewport_cmd(ViewportCommand::Close);
                            }
                        }
                    });
                });
            });
        }
    }

    fn recording_format_modal(&mut self, ctx: &Context) {
        Modal::new(Id::unique("recording-format")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(600.0));
            ui.heading("Recording format");
            ui.label("Sample rate");
            ui.add_enabled_ui(
                !self.recorder.controller.state.active && !self.recorder.controller.busy(),
                |ui| {
                    let width = (ui.available_width() - ui.spacing().item_spacing.x * 2.0) / 3.0;
                    for row in recording::SampleRate::ALL.chunks(3) {
                        ui.horizontal(|ui| {
                            for &rate in row {
                                let response = theme::button(
                                    ui,
                                    rate.label(),
                                    vec2(width, 48.0),
                                    self.recorder.preferences.sample_rate == rate,
                                );
                                self.hit(
                                    &format!("Recording rate {}", rate.label()),
                                    response.rect,
                                );
                                if response.clicked() {
                                    self.recorder.preferences.sample_rate = rate;
                                    self.save_recording_preferences();
                                }
                            }
                        });
                    }
                    ui.label("WAV bit depth");
                    ui.horizontal(|ui| {
                        let width = (ui.available_width() - ui.spacing().item_spacing.x) / 2.0;
                        for depth in [recording::BitDepth::Pcm16, recording::BitDepth::Pcm24] {
                            let response = theme::button(
                                ui,
                                depth.label(),
                                vec2(width, 48.0),
                                self.recorder.preferences.bit_depth == depth,
                            );
                            self.hit(&format!("Recording depth {}", depth.label()), response.rect);
                            if response.clicked() {
                                self.recorder.preferences.bit_depth = depth;
                                self.save_recording_preferences();
                            }
                        }
                    });
                },
            );
            ui.label(
                RichText::new(
                    "Auto prefers 48 kHz. Higher rates and bit depths use more disk space.",
                )
                .size(12.0)
                .color(MUTED),
            );
            if !self.recorder.error.is_empty() {
                ui.colored_label(RED, &self.recorder.error);
            }
            let done = theme::primary(ui, "Done", vec2(ui.available_width(), 48.0));
            self.hit("Recording format done", done.rect);
            if done.clicked() {
                self.recorder.format_picker = false;
            }
        });
    }
}

fn meters(ui: &mut Ui, state: &recording::State) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 78.0), Sense::click());
    let p = ui.painter();
    let left = rect.left() + 25.0;
    let right = rect.right() - 90.0;
    let x = |db: f32| lerp(left..=right, ((db + 60.0) / 60.0).clamp(0.0, 1.0));
    for db in [-60, -36, -24, -12, -6, 0] {
        p.text(
            pos2(x(db as f32), rect.top() + 3.0),
            Align2::CENTER_TOP,
            db.to_string(),
            FontId::proportional(10.0),
            MUTED,
        );
    }
    for (c, label) in ["L", "R"].into_iter().enumerate() {
        let y = rect.top() + 23.0 + c as f32 * 22.0;
        p.text(
            pos2(rect.left(), y + 7.0),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(14.0),
            TEXT,
        );
        let bar = Rect::from_min_max(pos2(left, y), pos2(right, y + 14.0));
        p.rect_filled(bar, 1, CARD);
        let peak = recording::db(state.peak[c]);
        for (lo, hi, color) in [
            (-60.0, -12.0, GREEN),
            (-12.0, -3.0, ORANGE),
            (-3.0, 0.0, RED),
        ] {
            if peak > lo {
                p.rect_filled(
                    Rect::from_min_max(pos2(x(lo), y), pos2(x(peak.min(hi)), y + 14.0)),
                    0,
                    color.gamma_multiply(0.55),
                );
            }
        }
        let rms = x(recording::db(state.rms[c]));
        p.rect_filled(
            Rect::from_min_max(pos2(left, y + 5.0), pos2(rms, y + 10.0)),
            0,
            GREEN,
        );
        if state.peak[c] > 0.000_001 {
            p.vline(x(peak), y..=y + 14.0, Stroke::new(2.0, TEXT));
        }
        let text = if state.clipped[c] {
            "CLIP".into()
        } else if peak <= -60.0 {
            "−∞ dBFS".into()
        } else {
            format!("{peak:.1} dBFS")
        };
        p.text(
            pos2(rect.right(), y + 7.0),
            Align2::RIGHT_CENTER,
            text,
            FontId::proportional(12.0),
            if state.clipped[c] { RED } else { MUTED },
        );
    }
    response.on_hover_text("Peak and RMS input levels. CLIP stays lit until you tap the meters. Adjust input gain on your mixer or sound card.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{click, frame};

    #[test]
    fn recording_survives_navigation_and_reconnect_and_guards_window_close() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
            let ctx = Context::default();
            let mut app = App::new(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            app.preview = Controller::test().0;
            let mut state = recording::State::default();
            state.active = true;
            state.recording = true;
            state.rate = 48_000;
            state.frames = 48_000 * 123;
            state.paths.push(PathBuf::from("/tmp/Session-001.wav"));
            app.recorder.controller.set_test_state(state);
            app.page = Page::Record;
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            assert!(app.recording_status().unwrap().0.contains("00:02:03"));
            for label in ["Recording start stop", "Recording monitor"] {
                let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                assert!(rect.bottom() <= size.y - 26.0);
                assert!(rect.right() <= size.x - 16.0);
            }
            click(&mut app, &ctx, size, "Players");
            assert_eq!(app.page, Page::Players);
            app.connect(&ctx);
            assert!(app.recorder.controller.state.recording);
            let mut input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                ..Default::default()
            };
            input
                .viewports
                .entry(ViewportId::ROOT)
                .or_default()
                .events
                .push(ViewportEvent::Close);
            let mut output = ctx.run_ui(input, |ui| app.render(ui));
            output.textures_delta.clear();
            assert!(app.recorder.confirm_close);
            assert!(
                output.viewport_output[&ViewportId::ROOT]
                    .commands
                    .contains(&ViewportCommand::CancelClose)
            );
            // A close request has not stopped capture or created another worker.
            assert!(app.recorder.controller.state.recording);
            assert!(!app.recorder.controller.busy());
        }
    }
    #[test]
    fn recorder_controls_and_fifth_tab_fit_touchscreens() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
            let ctx = Context::default();
            let mut app = App::new(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            app.preview = Controller::test().0;
            frame(&mut app, &ctx, size, vec![]);
            click(&mut app, &ctx, size, "Record");
            assert_eq!(app.page, Page::Record);
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            for label in [
                "Record",
                "Recording input",
                "Recording format",
                "Recording folder",
                "Recording start stop",
                "Recording monitor",
                "Recording meters",
            ] {
                let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                assert!(rect.height() >= 48.0, "{label}: {rect:?}");
                assert!(
                    Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                    "{label}: {rect:?}"
                );
            }
            click(&mut app, &ctx, size, "Recording folder");
            assert!(app.keyboard == Some(Field::RecordingDirectory));
            click(&mut app, &ctx, size, "Keyboard done");
            assert!(app.keyboard.is_none());
            click(&mut app, &ctx, size, "Recording format");
            assert!(app.recorder.format_picker);
            for _ in 0..3 {
                frame(&mut app, &ctx, size, vec![]);
            }
            for (label, rect) in &app.hits {
                if label.starts_with("Recording rate ")
                    || label.starts_with("Recording depth ")
                    || label == "Recording format done"
                {
                    assert!(rect.height() >= 48.0, "{label}: {rect:?}");
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains_rect(*rect),
                        "{label}: {rect:?}"
                    );
                }
            }
            click(&mut app, &ctx, size, "Recording rate 96 kHz");
            click(&mut app, &ctx, size, "Recording depth 16-bit");
            assert_eq!(
                app.recorder.preferences.sample_rate,
                recording::SampleRate::Hz96000
            );
            assert_eq!(
                app.recorder.preferences.bit_depth,
                recording::BitDepth::Pcm16
            );
            click(&mut app, &ctx, size, "Recording format done");
            assert!(!app.recorder.format_picker);
            for recording in [false, true] {
                let mut state = recording::State::default();
                state.active = true;
                state.recording = recording;
                app.recorder.controller.set_test_state(state);
                frame(&mut app, &ctx, size, vec![]);
                click(&mut app, &ctx, size, "Recording format");
                assert!(!app.recorder.format_picker, "Format changed during capture");
            }
        }
    }
}
