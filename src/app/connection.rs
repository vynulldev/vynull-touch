// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::settings::StartupPage;

#[cfg(test)]
mod tests;

impl From<StartupPage> for Page {
    fn from(page: StartupPage) -> Self {
        match page {
            StartupPage::Players => Self::Players,
            StartupPage::Collection => Self::Collection,
            StartupPage::Playlists => Self::Playlists,
        }
    }
}

impl App {
    pub(super) fn remember_server(&mut self) {
        if self.options.demo || self.remembered_server.as_ref() == Some(&self.options.server) {
            return;
        }
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_server(path, &self.options.server)
        {
            self.notice = format!("Connected, but could not save the server address: {error}");
            return;
        }
        self.remembered_server = Some(self.options.server.clone());
    }

    pub(super) fn reconnect_session(&mut self, ctx: &Context) {
        let pending = self.grid_editor.as_ref().is_some_and(|e| e.pending)
            || self.load_busy
            || self
                .editor
                .as_ref()
                .is_some_and(|editor| editor.pending && editor.saving)
            || self.pending_load.is_some()
            || self.cue_quick.is_some()
            || self
                .cue_editor
                .as_ref()
                .is_some_and(|editor| editor.pending);
        if self.page == Page::Detail {
            self.leave_inspect();
        }
        // A new session cannot reuse IDs, drafts or queued commands. Dropping
        // the old worker cancels its queue; no write is replayed on the new one.
        self.connect(ctx);
        let message = if pending {
            "Library session changed. Pending load/edit outcome is unknown; reload and review the affected player, track or library item. Select tracks again."
        } else {
            "Library session changed. Reloading tracks; select a track again before loading or editing."
        };
        self.session_notice = Some(message.into());
        self.notice = message.into();
    }

    fn connection_message(&self) -> String {
        if self.options.demo {
            return "Demo mode".into();
        }
        match self.connection {
            api::Connection::Connected => "Connected".into(),
            api::Connection::Connecting { attempt: 0 } => "Connecting to Vynull…".into(),
            api::Connection::Connecting { attempt } => format!("Reconnecting… attempt {attempt}"),
            api::Connection::Waiting { until, attempt } => {
                let seconds = until
                    .saturating_duration_since(Instant::now())
                    .as_secs_f32()
                    .ceil() as u64;
                format!("Connection unavailable · retry {attempt} in {seconds}s")
            }
        }
    }

    pub(super) fn connection_banner(&mut self, ui: &mut Ui) {
        if self.options.demo || matches!(self.connection, api::Connection::Connected) {
            return;
        }
        Frame::new()
            .fill(PANEL)
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let retry_width = 128.0;
                    ui.add_sized(
                        vec2(
                            (ui.available_width() - retry_width - ui.spacing().item_spacing.x)
                                .max(0.0),
                            44.0,
                        ),
                        Label::new(RichText::new(self.connection_message()).color(ORANGE))
                            .truncate(),
                    );
                    let retry = ui
                        .add_enabled_ui(
                            matches!(self.connection, api::Connection::Waiting { .. }),
                            |ui| theme::button(ui, "Retry now", vec2(retry_width, 44.0), false),
                        )
                        .inner;
                    self.hit("Connection retry", retry.rect);
                    if retry.clicked()
                        && let Some(worker) = &self.worker
                    {
                        worker.retry();
                    }
                });
            });
        ui.add_space(8.0);
        ui.ctx().request_repaint_after(Duration::from_millis(250));
    }

    pub(super) fn settings(&mut self, ui: &mut Ui, ctx: &Context) {
        ScrollArea::vertical()
            .id_salt("setup")
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.heading("Make yourself at home");
                ui.label(RichText::new("Connect this screen to your Vynull library.").size(14.0).color(MUTED));
                ui.add_space(10.0);
                ui.label(RichText::new("VYNULL SERVER").size(11.0).color(ORANGE));
                let response = ui.add_sized(
                    vec2(ui.available_width(), 48.0),
                    TextEdit::singleline(&mut self.server_draft).margin(vec2(12.0, 14.0)),
                );
                self.hit("Server address", response.rect);
                if response.clicked() { self.keyboard = Some(Field::Server); }
                ui.horizontal(|ui| {
                    let enabled = !self.load_busy && self.cue_quick.is_none()
                        && !self.cue_editor.as_ref().is_some_and(|editor| editor.pending);
                    let connect = ui.add_enabled_ui(enabled, |ui| theme::primary(ui, "Connect", vec2(130.0, 48.0))).inner;
                    self.hit("Server connect", connect.rect);
                    if connect.clicked() {
                        match api::validate_origin(self.server_draft.trim()) {
                            Ok(_) => {
                                self.options.server = self.server_draft.trim().to_owned();
                                self.server_draft = self.options.server.clone();
                                self.options.demo = false;
                                self.connect(ctx);
                            }
                            Err(error) => self.notice = error,
                        }
                    }
                    let demo = ui.add_enabled_ui(enabled, |ui| theme::button(ui, "Try demo", vec2(130.0, 48.0), false)).inner;
                    self.hit("Try demo", demo.rect);
                    if demo.clicked() {
                        self.options.demo = true;
                        self.connect(ctx);
                        self.page = Page::Collection;
                    }
                    let refresh = ui.add_enabled_ui(self.library_ready && !self.loading && !self.refresh.inflight, |ui| theme::button(ui, "Refresh", vec2(112.0, 48.0), false)).inner;
                    self.hit("Library refresh", refresh.rect);
                    if refresh.clicked() {
                        if self.options.demo { self.notice = "Demo collection is up to date".into(); }
                        else {
                            self.refresh.force = true;
                            self.refresh.next = Instant::now();
                            self.notice = "Refreshing library…".into();
                        }
                    }
                    let diagnostics = theme::button(ui, "Diagnostics", vec2(144.0, 48.0), false)
                        .on_hover_text("Connection status, versions and server logs");
                    self.hit("Diag", diagnostics.rect);
                    if diagnostics.clicked() {
                        self.page = Page::Diag;
                        self.keyboard = None;
                    }
                });
                if !self.refresh.error.is_empty() {
                    ui.colored_label(ORANGE, &self.refresh.error);
                } else if !self.options.demo && self.library_ready {
                    ui.label(RichText::new(if self.library_session.is_none() || self.refresh.tracks_revision.is_none() {
                        "Automatic refresh unavailable on this server. Use Refresh to check for changes."
                    } else if self.refresh.skip_cues {
                        "Library refreshes automatically. Use Refresh for cues on this server."
                    } else {
                        "Library and cues refresh automatically when you finish scrolling or editing."
                    }).size(12.0).color(MUTED));
                }
                ui.add_space(10.0);
                ui.label(RichText::new("STARTUP TAB").size(11.0).color(ORANGE));
                ui.horizontal(|ui| {
                    for page in [StartupPage::Players, StartupPage::Collection, StartupPage::Playlists] {
                        let choice = theme::button(ui, page.label(), vec2(130.0, 48.0), self.startup_page == page);
                        self.hit(&format!("Startup {}", page.label()), choice.rect);
                        if choice.clicked() {
                            self.startup_page = page;
                            if let Some(path) = &self.settings_path
                                && let Err(error) = settings::save_startup_page(path, page) {
                                self.notice = format!("Startup tab changed, but could not save it: {error}");
                            }
                        }
                    }
                });
                ui.label(RichText::new("Server saved after connecting. Launcher options override saved preferences.").size(12.0).color(MUTED));
                ui.add_space(10.0);
                ui.label(RichText::new("WAVEFORM STYLE").size(11.0).color(ORANGE));
                ui.horizontal(|ui| {
                    for style in crate::waveform::Style::ALL {
                        let choice = theme::button(ui, style.label(), vec2(112.0, 48.0), self.waveform_style == style);
                        self.hit(&format!("Waveform style {}", style.label()), choice.rect);
                        if choice.clicked() {
                            self.waveform_style = style;
                            if let Some(path) = &self.settings_path
                                && let Err(error) = settings::save_waveform_style(path, style) {
                                self.notice = format!("Waveform style changed, but could not save it: {error}");
                            }
                        }
                    }
                });
                ui.label(RichText::new("Library, Players and Inspect. Auto prefers 3-band; missing analysis uses RGB.").size(12.0).color(MUTED));
                ui.add_space(10.0);
                ui.label(RichText::new("FULL-TRACK WAVEFORM").size(11.0).color(ORANGE));
                let three_band = self.waveform_style == crate::waveform::Style::ThreeBand;
                let overview = if three_band { crate::waveform::Overview::Half } else { self.waveform_overview };
                ui.horizontal(|ui| {
                    for shape in [crate::waveform::Overview::Half, crate::waveform::Overview::Full] {
                        let choice = ui.add_enabled_ui(!three_band, |ui| {
                            theme::button(ui, shape.label(), vec2(112.0, 48.0), overview == shape)
                        }).inner;
                        self.hit(&format!("Waveform overview {}", shape.label()), choice.rect);
                        if choice.clicked() {
                            self.waveform_overview = shape;
                            if let Some(path) = &self.settings_path
                                && let Err(error) = settings::save_waveform_overview(path, shape) {
                                self.notice = format!("Full-track waveform changed, but could not save it: {error}");
                            }
                        }
                    }
                });
                ui.label(RichText::new(if three_band {
                    "3-band always uses Half. Your RGB/Blue choice is remembered."
                } else {
                    "Library, Players and Inspect. Auto uses Half whenever it displays 3-band."
                }).size(12.0).color(MUTED));
                if !self.connection_error.is_empty() { ui.colored_label(ORANGE, &self.connection_error); }
                ui.add_space(10.0);
                ui.label(RichText::new("PREVIEW AUDIO").size(11.0).color(ORANGE));
                ui.horizontal(|ui| {
                    let output = theme::button(ui, "Output…", vec2(130.0, 48.0), false);
                    self.hit("Preview output", output.rect);
                    if output.clicked() {
                        self.preview.pause();
                        self.audio_dialog = true;
                        self.scan_audio(ui.ctx());
                    }
                    let selected = self.preview.request.device.as_ref()
                        .map_or("System default", |device| device.name.as_str());
                    ui.add(Label::new(RichText::new(format!("{selected} · {:.0}%", self.preview.request.volume * 100.0)).size(14.0).color(MUTED)).truncate());
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if theme::button(ui, "Toggle fullscreen", vec2(182.0, 48.0), false).clicked() {
                        let fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));
                        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(!fullscreen));
                    }
                    let tags = theme::button(ui, "Manage tags…", vec2(180.0, 48.0), false);
                    self.hit("Manage tags", tags.rect);
                    if tags.clicked() { self.open_tag_manager(); }
                    if theme::button(ui, "Quit", vec2(90.0, 48.0), false).clicked() {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                });
            });
    }
}
