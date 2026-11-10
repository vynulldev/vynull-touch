// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    Options,
    api::{self, Command, Event, Worker},
    artwork, demo,
    library::{Filters, Order},
    model::*,
    preview::{self, Controller, Phase},
    settings,
    theme::{self, *},
};
use eframe::egui::*;
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
mod beat_jump_ui;
mod connection;
mod cue_ui;
mod diag_ui;
mod edit_ui;
mod inspect_cursor;
mod inspect_header;
mod library_ui;
mod load_ui;
mod loop_ui;
mod match_ui;
#[cfg(test)]
mod navigation_tests;
#[cfg(test)]
mod phrase_tests;
mod player_ui;
mod refresh;
mod track_scroll;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Collection,
    Playlists,
    Players,
    Match,
    Settings,
    Diag,
    Detail,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Search,
    Server,
}
pub struct App {
    options: Options,
    page: Page,
    inspect_origin: Option<Page>,
    worker: Option<Worker>,
    artwork: Option<artwork::Cache>,
    library_session: Option<String>,
    refresh: refresh::State,
    library_scroll: Option<Id>,
    preview: Controller,
    audio_dialog: bool,
    audio_devices: Vec<preview::OutputDevice>,
    audio_scan: Option<std::sync::mpsc::Receiver<Result<Vec<preview::OutputDevice>, String>>>,
    audio_scan_error: String,
    tracks: Vec<Track>,
    all_tracks: Vec<Track>,
    search_text: Vec<String>,
    visible: Vec<usize>,
    playlists: Vec<Playlist>,
    playlist_counts: api::playlist_counts::Counts,
    editor: Option<edit_ui::Manager>,
    edit_serial: u64,
    demo_edits: crate::editing::Demo,
    matching: crate::matching::State,
    folder: u32,
    source: Option<u32>,
    source_name: String,
    query: String,
    order: Order,
    filters: Filters,
    filter_revision: u64,
    sort_dialog: Option<Order>,
    filter_dialog: Option<library_ui::FilterDraft>,
    selected: Option<u32>,
    detail_track: Option<Track>,
    detail: Option<Detail>,
    detail_busy: bool,
    cue_editor: Option<crate::cues::Editor>,
    cue_quick: Option<crate::cues::Editor>,
    cue_press: Option<cue_ui::PadPress>,
    loop_controls: bool,
    loop_error: String,
    beat_jump_beats: u8,
    inspect_quantize: bool,
    demo_cues: std::collections::HashMap<u32, Vec<Cue>>,
    position: f64,
    zoom: f64,
    waveform_style: crate::waveform::Style,
    waveform_overview: crate::waveform::Overview,
    settings_path: Option<PathBuf>,
    show_grid: bool,
    wave_drag_start: Option<f64>,
    status: Status,
    deck_cache: Option<crate::decks::Cache>,
    diagnostics: diag_ui::Diagnostics,
    deck_pair: [u8; 2],
    deck_picker: Option<[u8; 2]>,
    deck_focus: Option<u8>,
    deck_span: f64,
    deck_compare: bool,
    deck_clocks: std::collections::HashMap<crate::decks::Key, crate::decks::Clock>,
    demo_decks: std::collections::HashMap<u32, std::sync::Arc<crate::decks::Assets>>,
    demo_started: Instant,
    linked: bool,
    last_status: Option<Instant>,
    loading: bool,
    notice: String,
    connection_error: String,
    connection: api::Connection,
    library_ready: bool,
    session_notice: Option<String>,
    remembered_server: Option<String>,
    startup_page: settings::StartupPage,
    server_draft: String,
    keyboard: Option<Field>,
    symbols: bool,
    load_dialog: bool,
    load_confirmation: Option<load_ui::Confirmation>,
    load_busy: bool,
    pending_load: Option<(u32, u8, Instant)>,
    token: u64,
    frames: u32,
    screenshot_requested: bool,
    #[cfg(test)]
    hits: Vec<(String, Rect)>,
}

impl App {
    pub fn new(ctx: &Context, mut options: Options, settings_path: Option<PathBuf>) -> Self {
        theme::configure(ctx, options.scale);
        let saved = settings::load(settings_path.as_deref());
        if !options.server_given
            && let Some(server) = &saved.server
        {
            options.server = server.clone();
        }
        let page = options.startup_page.unwrap_or(saved.startup_page).into();
        let mut preview = Controller::new(ctx.clone());
        preview.request.device = saved.audio_output;
        preview.request.volume = saved.audio_volume;
        let mut app = Self {
            server_draft: options.server.clone(),
            options,
            page,
            inspect_origin: None,
            worker: None,
            artwork: None,
            library_session: None,
            refresh: refresh::State::default(),
            library_scroll: None,
            preview,
            audio_dialog: false,
            audio_devices: vec![],
            audio_scan: None,
            audio_scan_error: String::new(),
            tracks: vec![],
            all_tracks: vec![],
            search_text: vec![],
            visible: vec![],
            playlists: vec![],
            playlist_counts: Default::default(),
            editor: None,
            edit_serial: 0,
            demo_edits: Default::default(),
            matching: Default::default(),
            folder: 0,
            source: None,
            source_name: "All tracks".into(),
            query: String::new(),
            order: saved.library_order,
            filters: Filters::default(),
            filter_revision: 0,
            sort_dialog: None,
            filter_dialog: None,
            selected: None,
            detail_track: None,
            detail: None,
            detail_busy: false,
            cue_editor: None,
            cue_quick: None,
            cue_press: None,
            loop_controls: false,
            loop_error: String::new(),
            beat_jump_beats: saved.beat_jump_beats,
            inspect_quantize: saved.inspect_quantize,
            demo_cues: Default::default(),
            position: 0.0,
            zoom: saved.waveform_zoom,
            waveform_style: saved.waveform_style,
            waveform_overview: saved.waveform_overview,
            settings_path,
            show_grid: true,
            wave_drag_start: None,
            status: Status::default(),
            deck_cache: None,
            diagnostics: diag_ui::Diagnostics::default(),
            deck_pair: saved.players.pair,
            deck_picker: None,
            deck_focus: None,
            deck_span: saved.players.waveform_span,
            deck_compare: saved.players.compare,
            deck_clocks: Default::default(),
            demo_decks: Default::default(),
            demo_started: Instant::now(),
            linked: false,
            last_status: None,
            loading: false,
            notice: String::new(),
            connection_error: String::new(),
            connection: api::Connection::Connected,
            library_ready: false,
            session_notice: None,
            remembered_server: saved.server,
            startup_page: saved.startup_page,
            keyboard: None,
            symbols: false,
            load_dialog: false,
            load_confirmation: None,
            load_busy: false,
            pending_load: None,
            token: 0,
            frames: 0,
            screenshot_requested: false,
            #[cfg(test)]
            hits: vec![],
        };
        app.connect(ctx);
        app
    }

    fn connect(&mut self, ctx: &Context) {
        self.refresh = refresh::State::default();
        self.library_scroll = None;
        self.library_ready = false;
        self.session_notice = None;
        self.connection = if self.options.demo {
            api::Connection::Connected
        } else {
            api::Connection::Connecting { attempt: 0 }
        };
        self.inspect_origin = None;
        self.filters = Filters::default();
        self.filter_revision = self.filter_revision.wrapping_add(1);
        self.sort_dialog = None;
        self.filter_dialog = None;
        self.loop_controls = false;
        self.loop_error.clear();
        self.cue_editor = None;
        self.cue_quick = None;
        self.cue_press = None;
        self.demo_cues.clear();
        self.deck_cache = None;
        self.diagnostics = diag_ui::Diagnostics::default();
        self.demo_decks.clear();
        self.deck_picker = None;
        self.deck_focus = None;
        self.deck_clocks.clear();
        self.demo_started = Instant::now();
        self.preview.stop();
        self.library_session = None;
        self.audio_dialog = false;
        self.worker = None;
        self.artwork = None;
        self.token += 1;
        self.selected = None;
        self.detail_track = None;
        self.detail = None;
        self.detail_busy = false;
        self.source = None;
        self.source_name = "All tracks".into();
        self.folder = 0;
        self.tracks.clear();
        self.all_tracks.clear();
        self.playlists.clear();
        self.playlist_counts = Default::default();
        self.editor = None;
        self.demo_edits = Default::default();
        self.matching = Default::default();
        self.status = Status::default();
        self.last_status = None;
        self.linked = false;
        self.connection_error.clear();
        self.load_busy = false;
        self.load_dialog = false;
        self.load_confirmation = None;
        self.pending_load = None;
        self.query.clear();
        self.keyboard = None;
        if self.options.demo {
            self.library_ready = true;
            self.all_tracks = demo::tracks();
            self.demo_cues = self
                .all_tracks
                .iter()
                .map(|track| (track.id, demo::cues()))
                .collect();
            self.tracks = self.all_tracks.clone();
            self.playlists = demo::playlists();
            self.status = demo::status();
            self.linked = true;
            self.loading = false;
            self.notice = "Demo collection · fictional tracks and players".into();
        } else {
            self.loading = true;
            self.notice = "Connecting to Vynull…".into();
            self.worker = Some(Worker::spawn(self.options.server.clone(), ctx.clone()));
        }
        self.reindex();
    }

    fn reindex(&mut self) {
        self.matching.dirty = true;
        if let Some(draft) = &mut self.filter_dialog {
            draft.dirty = true;
        }
        self.search_text = self.tracks.iter().map(Track::search_text).collect();
        self.filter();
    }
    fn filter(&mut self) {
        let query = self.query.to_lowercase();
        let words: Vec<_> = query.split_whitespace().collect();
        self.visible = (0..self.tracks.len())
            .filter(|&i| {
                words.iter().all(|word| self.search_text[i].contains(word))
                    && self.filters.matches(&self.tracks[i])
            })
            .collect();
        self.order.apply(&self.tracks, &mut self.visible);
        if self
            .selected
            .is_some_and(|id| !self.visible.iter().any(|&i| self.tracks[i].id == id))
        {
            self.selected = None;
            self.load_dialog = false;
            self.load_confirmation = None;
        }
    }
    fn send(&mut self, command: Command) -> bool {
        self.refresh.generation = self.refresh.generation.wrapping_add(1);
        if !self.options.demo && !self.library_ready {
            self.notice = "Waiting for the library connection; request was not sent".into();
            return false;
        }
        if let Some(worker) = &self.worker
            && worker.commands.try_send(command).is_ok()
        {
            return true;
        }
        self.notice = "Request queue is busy; try again shortly".into();
        false
    }
    fn fresh(&self) -> bool {
        self.options.demo
            || self
                .last_status
                .is_some_and(|time| time.elapsed() < Duration::from_secs(4))
    }
    fn selected_track(&self) -> Option<&Track> {
        if self.page == Page::Match {
            let id = self.matching.selected?;
            let item = self.matching.results.iter().find(|m| m.id == id)?;
            self.all_tracks
                .iter()
                .find(|t| t.id == id && t.file_path == item.path)
        } else if self.page == Page::Detail {
            self.detail_track.as_ref()
        } else {
            self.selected
                .and_then(|id| self.tracks.iter().find(|t| t.id == id))
        }
    }
    fn inspect(&mut self, track: Track) {
        if self.page != Page::Detail {
            // Browsing and player-view state stays in place while inspecting.
            // Opening another track within Inspect must retain the original page.
            self.inspect_origin = Some(self.page);
        }
        self.loop_controls = false;
        self.loop_error.clear();
        self.cue_editor = None;
        self.cue_quick = None;
        self.cue_press = None;
        self.preview.load(
            (track.id, track.duration),
            self.options.server.clone(),
            self.library_session.clone(),
            self.options.demo,
        );
        self.token += 1;
        self.position = 0.0;
        self.wave_drag_start = None;
        self.page = Page::Detail;
        self.keyboard = None;
        self.detail_track = Some(track.clone());
        self.detail = None;
        if self.options.demo {
            self.detail = Some(demo::detail(&track));
            if let Some(cues) = self.demo_cues.get(&track.id) {
                self.detail.as_mut().unwrap().cues = cues.clone();
            }
            self.detail_busy = false;
        } else {
            self.detail_busy = self.send(Command::Detail(track.id, self.token));
        }
    }

    fn leave_inspect(&mut self) {
        self.page = self.inspect_origin.take().unwrap_or(Page::Collection);
    }

    fn navigation_page(&self) -> Page {
        if self.page == Page::Detail {
            match self.inspect_origin.unwrap_or(Page::Collection) {
                Page::Match => Page::Players,
                page => page,
            }
        } else if self.page == Page::Diag {
            Page::Settings
        } else if self.page == Page::Match {
            Page::Players
        } else {
            self.page
        }
    }

    fn source(&mut self, playlist: Option<Playlist>) {
        self.token += 1;
        self.source = playlist.as_ref().map(|p| p.id);
        self.source_name = playlist
            .as_ref()
            .map_or("All tracks".into(), |p| p.name.clone());
        self.selected = None;
        self.query.clear();
        self.page = Page::Collection;
        self.keyboard = None;
        self.tracks.clear();
        if let Some(playlist) = playlist {
            if self.options.demo {
                self.tracks = playlist
                    .track_ids
                    .iter()
                    .filter_map(|id| self.all_tracks.iter().find(|t| t.id == *id).cloned())
                    .collect();
            } else {
                self.loading = self.send(Command::Playlist(playlist.id, self.token));
            }
        } else {
            self.tracks = self.all_tracks.clone();
            self.loading = false;
        }
        self.reindex();
    }

    fn set_zoom(&mut self, zoom: f64) {
        if self.zoom == zoom {
            return;
        }
        self.zoom = zoom;
        self.wave_drag_start = None;
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_zoom(path, zoom)
        {
            self.notice = format!("Zoom changed, but could not save the setting: {error}");
        }
    }

    fn poll(&mut self, ctx: &Context) {
        let events: Vec<_> = self
            .worker
            .as_ref()
            .map(|w| w.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            self.diagnostics.requests.observe(&event);
            let mut error = None;
            match event {
                Event::Edit(token, result) => {
                    if let Err(e) = &result
                        && e.contains("Library changed")
                    {
                        self.reconnect_session(ctx);
                        break;
                    }
                    self.edit_response(token, result);
                }
                Event::Connection(state) => self.connection = state,
                Event::Library(result) => {
                    self.loading = false;
                    match result {
                        Ok(library) => {
                            self.refresh.checked = Some(Instant::now());
                            self.refresh.tracks_revision = library.revision;
                            self.library_ready = true;
                            self.deck_cache = Some(crate::decks::Cache::new(
                                self.options.server.clone(),
                                library.session.clone(),
                                ctx.clone(),
                            ));
                            self.library_session = library.session.clone();
                            self.artwork = Some(artwork::Cache::new(
                                &self.options.server,
                                library.session,
                                ctx.clone(),
                            ));
                            self.all_tracks = library.tracks;
                            self.playlists = library.playlists;
                            if self.source.is_none() {
                                self.tracks = self.all_tracks.clone();
                                self.reindex();
                            }
                            self.notice = self.session_notice.clone().unwrap_or_else(|| {
                                format!("Collection ready · {} tracks", self.all_tracks.len())
                            });
                            self.connection_error.clear();
                            self.remember_server();
                        }
                        Err(e) => {
                            if !self.library_ready {
                                self.connection_error = e.clone();
                            }
                            error = Some(e);
                        }
                    }
                }
                Event::Playlist(id, token, result)
                    if self.source == Some(id) && token == self.token =>
                {
                    self.loading = false;
                    match result {
                        Ok(tracks) => {
                            self.playlist_counts.remember(id, tracks.len());
                            self.tracks = tracks;
                            self.reindex();
                        }
                        Err(e) => error = Some(e),
                    }
                }
                Event::Detail(id, token, result)
                    if token == self.token
                        && self.detail_track.as_ref().is_some_and(|t| t.id == id) =>
                {
                    self.detail_busy = false;
                    match result {
                        Ok(detail) => {
                            if detail.cues_ready {
                                self.refresh.remember_cues(id, &detail.cues);
                            }
                            self.detail = Some(detail);
                        }
                        Err(e) => error = Some(e),
                    }
                }
                Event::Cues(id, token, written, result)
                    if token == self.token
                        && self.detail_track.as_ref().is_some_and(|t| t.id == id) =>
                {
                    let quick = self.cue_quick.take();
                    if result.is_err() && quick.is_some() {
                        // Keep the exact attempted cue for an explicit retry or reload.
                        self.preview.pause();
                        self.cue_editor = quick;
                    }
                    if let Some(editor) = &mut self.cue_editor {
                        editor.pending = false;
                    }
                    match result {
                        Ok(saved) => {
                            self.refresh.remember_cues(id, &saved.cues);
                            if let Some(cache) = &mut self.deck_cache {
                                cache.invalidate(id);
                            }
                            if let Some(detail) = &mut self.detail {
                                detail.cues = saved.cues;
                                detail.cues_ready = true;
                            }
                            self.notice = if !written {
                                "Cues reloaded".into()
                            } else if saved.persisted {
                                "Cue saved to disk".into()
                            } else {
                                "Cue applied and verified; daemon does not confirm disk persistence"
                                    .into()
                            };
                            if written {
                                self.cue_editor = None;
                            } else if let (Some(editor), Some(detail), Some(track)) =
                                (&self.cue_editor, &self.detail, &self.detail_track)
                            {
                                self.cue_editor = Some(crate::cues::Editor::new(
                                    &detail.cues,
                                    editor.draft.number,
                                    self.position,
                                    self.preview_duration(track),
                                    &detail.beat_grid,
                                ));
                            }
                        }
                        Err(e) => {
                            if let Some(editor) = &mut self.cue_editor {
                                editor.error = e.clone();
                            }
                            error = Some(e);
                        }
                    }
                }
                Event::Status(sampled, result) => match result {
                    Ok((status, linked)) => {
                        self.connection = api::Connection::Connected;
                        self.status = status;
                        self.linked = linked;
                        self.last_status = Some(sampled);
                        if !self.connection_error.is_empty() && self.notice == self.connection_error
                        {
                            self.notice = "Connection restored".into();
                        }
                        self.connection_error.clear();
                    }
                    Err(e) => {
                        self.last_status = None;
                        self.connection_error = e.clone();
                        error = Some(e);
                    }
                },
                Event::Load(result) => {
                    self.load_busy = false;
                    match result {
                        Ok((id, player)) => {
                            self.pending_load = Some((id, player, Instant::now()));
                            self.notice =
                                format!("Load sent to Player {player} · waiting for its status");
                        }
                        Err(e) => error = Some(e),
                    }
                }
                Event::Refresh(request, result) => {
                    self.refresh.inflight = false;
                    match result {
                        Ok(snapshot) => self.refresh.pending = Some((request, *snapshot)),
                        Err(e) if e.contains("Library changed") => error = Some(e),
                        Err(e) => self.refresh_failed(request.force, e),
                    }
                }
                _ => (),
            }
            if let Some(error) = error {
                if error.contains("Library changed") {
                    self.reconnect_session(ctx);
                    break;
                }
                self.notice = error;
            }
        }
        self.refresh_tick(ctx);
        if let Some((id, number, started)) = self.pending_load {
            if self.fresh()
                && self.status.players.iter().any(|p| {
                    p.device_number == number
                        && p.track_id == id
                        && !p.external
                        && p.source.is_empty()
                        && matches!(p.play_state, 3..=9 | 0x11)
                })
            {
                self.notice = format!("Player {number} confirmed track loaded");
                self.pending_load = None;
            } else if started.elapsed() > Duration::from_secs(6) {
                self.notice =
                    format!("Load sent; Player {number} has not confirmed. Check the deck.");
                self.pending_load = None;
            }
        }
    }

    fn hit(&mut self, label: &str, rect: Rect) {
        #[cfg(test)]
        self.hits.push((label.into(), rect));
        #[cfg(not(test))]
        let _ = (label, rect);
    }

    fn header(&mut self, root: &mut Ui) {
        Panel::top("header")
            .exact_size(52.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(16, 0)),
            )
            .show(root, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new("vynull").size(29.0).color(ORANGE));
                    if self.page != Page::Collection || ui.max_rect().width() >= 960.0 {
                        ui.add_space(8.0);
                        ui.label(RichText::new("TOUCH").size(11.0).color(MUTED));
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let active = self.navigation_page() == Page::Settings;
                        let settings = theme::button(ui, "", vec2(48.0, 48.0), active)
                            .on_hover_text("Settings");
                        settings.widget_info(|| {
                            WidgetInfo::labeled(Role::Button, ui.is_enabled(), "Settings")
                        });
                        theme::icon(
                            ui.painter(),
                            settings.rect.center(),
                            3,
                            if active { ORANGE } else { MUTED },
                        );
                        self.hit("Setup", settings.rect);
                        if settings.clicked() {
                            self.page = Page::Settings;
                            self.keyboard = None;
                        }
                        let label = if self.options.demo {
                            "DEMO"
                        } else if self.fresh() {
                            "CONNECTED"
                        } else if self.loading
                            || matches!(self.connection, api::Connection::Connecting { .. })
                        {
                            "CONNECTING"
                        } else {
                            "OFFLINE"
                        };
                        let status =
                            ui.label(RichText::new(label).size(12.0).color(if self.fresh() {
                                GREEN
                            } else {
                                ORANGE
                            }));
                        self.hit("Connection status", status.rect);
                        if self.fresh() {
                            let players = ui.label(
                                RichText::new(format!("{} players", self.status.players.len()))
                                    .size(13.0)
                                    .color(MUTED),
                            );
                            self.hit("Player count", players.rect);
                        }
                        if self.page == Page::Collection {
                            ui.allocate_ui_with_layout(
                                vec2(ui.available_width(), 48.0),
                                Layout::left_to_right(Align::Center),
                                |ui| self.collection_toolbar(ui),
                            );
                        } else if self.page == Page::Playlists {
                            let manage = theme::button(ui, "Manage…", vec2(130.0, 48.0), false);
                            self.hit("Manage playlists", manage.rect);
                            if manage.clicked() {
                                self.open_playlist_manager(None);
                            }
                        } else if self.page == Page::Players {
                            let matches =
                                theme::button(ui, "Match a player", vec2(172.0, 48.0), false);
                            self.hit("Match a player", matches.rect);
                            if matches.clicked() {
                                self.open_matches();
                            }
                        }
                    });
                });
            });
    }

    fn navigation(&mut self, root: &mut Ui) {
        Panel::left("navigation")
            .exact_size(88.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(6, 12)),
            )
            .show(root, |ui| {
                for (page, label, icon) in [
                    (Page::Players, "Players", 2),
                    (Page::Collection, "Collection", 0),
                    (Page::Playlists, "Playlists", 1),
                ]
                .iter()
                {
                    let active = self.navigation_page() == *page;
                    let (rect, response) = ui.allocate_exact_size(vec2(76.0, 68.0), Sense::click());
                    if active || response.hovered() {
                        ui.painter()
                            .rect_filled(rect, 8, if active { SELECTED } else { CARD });
                    }
                    let color = if active { ORANGE } else { MUTED };
                    theme::icon(
                        ui.painter(),
                        rect.center_top() + vec2(0.0, 23.0),
                        *icon,
                        color,
                    );
                    ui.painter().text(
                        rect.center_bottom() - vec2(0.0, 16.0),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(12.0),
                        color,
                    );
                    self.hit(label, rect);
                    if response.clicked() {
                        self.page = *page;
                        self.keyboard = None;
                    }
                }
            });
    }

    pub fn render(&mut self, root: &mut Ui) {
        let ctx = root.ctx().clone();
        let popup_open = Popup::is_any_open(&ctx);
        self.poll(&ctx);
        if self.page != Page::Diag {
            self.diagnostics
                .logs
                .update(&ctx, &self.options.server, self.options.demo, false);
        }
        if self.preview.poll()
            && !self.preview.request.dragging
            && matches!(
                self.preview.state.phase,
                Phase::Playing | Phase::Buffering | Phase::Paused | Phase::Ended
            )
        {
            self.position = self.preview.state.position;
        }
        if let Some(result) = self
            .audio_scan
            .as_ref()
            .and_then(|scan| scan.try_recv().ok())
        {
            self.audio_scan = None;
            match result {
                Ok(devices) => {
                    self.audio_devices = devices;
                    self.audio_scan_error.clear();
                }
                Err(e) => self.audio_scan_error = e,
            }
        }
        if let Some(artwork) = &mut self.artwork {
            artwork.poll(&ctx);
        }
        let keys: std::collections::HashSet<_> = self
            .status
            .players
            .iter()
            .filter_map(|p| {
                crate::decks::Key::new(
                    self.status
                        .peers
                        .iter()
                        .find(|peer| peer.device_number == p.device_number),
                    p,
                )
            })
            .collect();
        self.deck_clocks.retain(|key, _| keys.contains(key));
        if let Some(cache) = &mut self.deck_cache {
            cache.retain(&keys);
            cache.poll(&ctx);
        }
        #[cfg(test)]
        self.hits.clear();
        self.header(root);
        Panel::bottom("status")
            .exact_size(26.0)
            .frame(
                Frame::new()
                    .fill(PANEL)
                    .inner_margin(Margin::symmetric(12, 3)),
            )
            .show(root, |ui| {
                let message = if self.page == Page::Detail && !self.loop_error.is_empty() {
                    self.loop_error.clone()
                } else if self.page == Page::Detail && self.cue_quick.is_some() {
                    "Saving and verifying hot cue…".to_owned()
                } else if self.page == Page::Detail {
                    match self.preview.state.phase {
                        Phase::Loading => "Preparing audio… Tap Pause to cancel".to_owned(),
                        Phase::Buffering => "Buffering preview audio…".to_owned(),
                        Phase::Playing => format!(
                            "Preview · {}",
                            self.preview
                                .state
                                .output_name
                                .as_deref()
                                .unwrap_or("System default")
                        ),
                        Phase::Ended => "Preview finished · tap Play to restart".to_owned(),
                        Phase::Error => self.preview.state.error.clone().unwrap_or_default(),
                        _ => self.notice.clone(),
                    }
                } else {
                    self.notice.clone()
                };
                ui.add(Label::new(RichText::new(&message).size(12.0).color(MUTED)).truncate())
                    .on_hover_text(message);
            });
        self.keyboard(root);
        if self.keyboard.is_none() {
            self.navigation(root);
        }
        CentralPanel::default()
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(
                16,
                if self.page == Page::Detail { 8 } else { 16 },
            )))
            .show(root, |ui| {
                self.connection_banner(ui);
                match self.page {
                    Page::Collection => self.collection(ui),
                    Page::Playlists => self.playlists(ui),
                    Page::Players => self.players(ui),
                    Page::Match => self.match_view(ui),
                    Page::Settings => self.settings(ui, &ctx),
                    Page::Diag => self.diagnostics(ui),
                    Page::Detail => self.detail(ui),
                }
            });
        let confirming_load = self.load_confirmation.is_some();
        if confirming_load {
            self.preview.pause();
            self.confirm_load_modal(&ctx);
        } else if self.load_dialog {
            self.preview.pause();
            self.load_modal(&ctx);
        }
        if self.audio_dialog {
            self.audio_modal(&ctx);
        }
        if self.cue_editor.is_some() {
            self.cue_modal(&ctx);
        }
        if self.deck_picker.is_some() && self.page == Page::Players {
            self.player_pair_modal(&ctx);
        }
        if self.page == Page::Collection {
            if self.sort_dialog.is_some() {
                self.sort_modal(&ctx);
            }
            if self.filter_dialog.is_some() {
                self.filter_modal(&ctx);
            }
        } else {
            self.sort_dialog = None;
            self.filter_dialog = None;
        }
        let editing = self.editor.is_some();
        if editing {
            self.edit_dialog(&ctx);
        }
        if !editing && !popup_open && ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.sort_dialog.is_some() {
                self.sort_dialog = None;
            } else if self.filter_dialog.is_some() {
                self.filter_dialog = None;
            } else if let Some(editor) = &mut self.cue_editor {
                if editor.typing {
                    editor.typing = false;
                } else if !editor.pending {
                    self.cue_editor = None;
                }
            } else if self.audio_dialog {
                self.audio_dialog = false;
            } else if confirming_load {
                self.load_confirmation = None;
            } else if self.load_dialog {
                self.load_dialog = false;
            } else if self.deck_picker.is_some() {
                self.deck_picker = None;
            } else if self.keyboard.is_some() {
                self.keyboard = None;
            } else if self.page == Page::Detail {
                self.leave_inspect();
            } else if self.page == Page::Diag {
                self.page = Page::Settings;
            } else if self.page == Page::Match {
                self.page = Page::Players;
            } else if self.page == Page::Players && self.deck_focus.is_some() {
                self.deck_focus = None;
            } else {
                ctx.send_viewport_cmd(ViewportCommand::Fullscreen(false));
            }
        }
        if self.page != Page::Detail {
            self.inspect_origin = None;
            self.preview.stop();
            self.cue_press = None;
        }
        if self.page != Page::Settings {
            self.audio_dialog = false;
        }
        if self.page != Page::Players {
            self.deck_picker = None;
        }
        // Sleeping between polls avoids continuous redraw on a small device.
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    fn collection(&mut self, ui: &mut Ui) {
        let searching = self.keyboard == Some(Field::Search);
        if searching {
            ui.label(
                RichText::new(format!(
                    "{} matching tracks · tap a track to select",
                    self.visible.len()
                ))
                .size(13.0)
                .color(MUTED),
            );
        } else {
            self.filter_summary(ui);
        }
        let list_height = (ui.available_height() - if searching { 0.0 } else { 64.0 }).max(20.0);
        let mut inspect = self.library_table(ui, list_height);
        if searching {
            return;
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if self.source.is_some() {
                let back =
                    theme::button(ui, "‹", vec2(48.0, 48.0), false).on_hover_text("All tracks");
                self.hit("All tracks", back.rect);
                if back.clicked() {
                    self.source(None);
                }
            }
            let selected = self.selected_track().cloned();
            let title_width =
                (ui.available_width() - if selected.is_some() { 286.0 } else { 0.0 }).max(30.0);
            let count = if self.filters.count() > 0 || !self.query.trim().is_empty() {
                format!("{} / {} tracks", self.visible.len(), self.tracks.len())
            } else {
                format!("{} tracks", self.visible.len())
            };
            let (rect, _) = ui.allocate_exact_size(vec2(title_width, 48.0), Sense::hover());
            theme::text(
                ui.painter(),
                Rect::from_min_max(rect.min, rect.right_top() + vec2(0.0, 20.0)),
                format!("{} · {count}", self.source_name),
                12.0,
                MUTED,
            );
            theme::text(
                ui.painter(),
                Rect::from_min_max(rect.left_top() + vec2(0.0, 20.0), rect.max),
                selected.as_ref().map_or_else(
                    || "Select a track to inspect or load to a player".into(),
                    |track| format!("{} · {}", track.title, track.artist),
                ),
                15.0,
                if selected.is_some() { TEXT } else { MUTED },
            );
            if let Some(track) = selected {
                let add = theme::button(ui, "+", vec2(48.0, 48.0), false)
                    .on_hover_text("Add to playlist");
                self.hit("Add to playlist", add.rect);
                if add.clicked() {
                    self.open_playlist_manager(Some(track.clone()));
                }
                let response = theme::button(ui, "Inspect", vec2(96.0, 48.0), false);
                self.hit("Inspect", response.rect);
                if response.clicked() {
                    inspect = Some(track);
                }
                let enabled = self.fresh() && !self.load_busy && self.pending_load.is_none();
                if ui
                    .add_enabled_ui(enabled, |ui| {
                        theme::primary(ui, "Load to…", vec2(116.0, 48.0))
                    })
                    .inner
                    .clicked()
                {
                    self.load_dialog = true;
                }
            }
        });
        if let Some(track) = inspect {
            self.inspect(track);
        }
    }

    fn playlists(&mut self, ui: &mut Ui) {
        self.playlist_counts.poll(&self.playlists);
        if self.folder != 0 {
            let back = theme::button(ui, "‹ Back", vec2(96.0, 48.0), false);
            self.hit("Playlist folder back", back.rect);
            if back.clicked() {
                self.folder = self
                    .playlists
                    .iter()
                    .find(|p| p.id == self.folder)
                    .map_or(0, |p| p.parent_id);
            }
        }
        let playlists: Vec<_> = self
            .playlists
            .iter()
            .filter(|p| p.parent_id == self.folder)
            .cloned()
            .collect();
        if playlists.is_empty() {
            ui.label("No playlists in this folder.");
        }
        ScrollArea::vertical()
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for playlist in playlists {
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 68.0), Sense::click());
                    if playlist.is_smart
                        && !playlist.is_folder
                        && !self.options.demo
                        && self.library_ready
                        && self.fresh()
                        && ui.is_rect_visible(rect)
                    {
                        self.playlist_counts.request(
                            playlist.id,
                            &self.options.server,
                            self.library_session.as_deref(),
                            ui.ctx(),
                        );
                    }
                    ui.painter().rect_filled(rect, 8, CARD);
                    theme::playlist_icon(
                        ui.painter(),
                        rect.left_center() + vec2(30.0, 0.0),
                        playlist.is_folder,
                        playlist.is_smart,
                        ORANGE,
                    );
                    theme::text(
                        ui.painter(),
                        Rect::from_min_max(
                            rect.min + vec2(60.0, 10.0),
                            rect.right_top() + vec2(-35.0, 34.0),
                        ),
                        &playlist.name,
                        18.0,
                        TEXT,
                    );
                    theme::text(
                        ui.painter(),
                        Rect::from_min_max(
                            rect.min + vec2(60.0, 36.0),
                            rect.right_bottom() + vec2(-35.0, -8.0),
                        ),
                        self.playlist_subtitle(&playlist),
                        13.0,
                        MUTED,
                    );
                    ui.painter().text(
                        rect.right_center() - vec2(18.0, 0.0),
                        Align2::CENTER_CENTER,
                        "›",
                        FontId::proportional(24.0),
                        MUTED,
                    );
                    self.hit(&playlist.name, rect);
                    if response.clicked() {
                        if playlist.is_folder {
                            self.folder = playlist.id;
                        } else {
                            self.source(Some(playlist));
                        }
                    }
                }
            });
    }

    fn playlist_subtitle(&self, playlist: &Playlist) -> String {
        if playlist.is_folder {
            return "Folder".into();
        }
        let smart = playlist.is_smart && !self.options.demo;
        let count = if smart {
            self.playlist_counts.count(playlist.id)
        } else {
            Some(playlist.track_ids.len())
        };
        let failed = smart && self.playlist_counts.failed(playlist.id);
        let total = match count {
            Some(n) => format!(
                "{n} {}{}",
                if n == 1 { "track" } else { "tracks" },
                if failed { " (last known)" } else { "" },
            ),
            None if failed => "Count unavailable".into(),
            None => "Counting…".into(),
        };
        if playlist.is_smart {
            format!("Smart playlist · {total}")
        } else {
            total
        }
    }

    fn detail(&mut self, ui: &mut Ui) {
        let Some(track) = self.detail_track.clone() else {
            self.leave_inspect();
            return;
        };
        self.inspect_header(ui, &track);
        // Tighten vertical gaps on small screens to make room for the taller
        // overview while keeping the detailed waveform and touch controls usable.
        // Measure before scrolling so the waveform heights stay stable.
        // Leave a little room for pixel rounding across the stacked rows.
        let available = (ui.available_height() - 4.0).max(0.0);
        let phrase_height = self.inspect_phrase_height();
        // Use spare height for phrase strips on larger screens. At 800×480 the
        // strips share the waveform panels so all touch controls remain visible.
        let extra =
            ((available - 64.0 - 88.0 - 3.0 * 48.0 - 4.0 * 2.0) / 2.0).clamp(0.0, phrase_height);
        let overview_height = 64.0 + extra;
        let gap =
            ((available - overview_height - (88.0 + extra) - 3.0 * 48.0) / 4.0).clamp(2.0, 6.0);
        let detail_height = (available - overview_height - 3.0 * 48.0 - 4.0 * gap)
            .clamp(88.0 + extra, 176.0 + phrase_height);
        // Keep transport and track actions reachable while extra cues and
        // warnings scroll in the remaining space above them.
        Panel::bottom("inspect-actions")
            .exact_size(48.0)
            .resizable(false)
            .show_separator_line(false)
            .frame(Frame::new().fill(BG))
            .show(ui, |ui| self.inspect_actions(ui, &track));
        ScrollArea::vertical()
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false])
            .max_height((ui.available_height() - gap).max(0.0))
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = gap;
                let duration = self.preview_duration(&track);
                let (overview, overview_response) = ui.allocate_exact_size(
                    vec2(ui.available_width(), overview_height),
                    Sense::click_and_drag(),
                );
                self.hit("Full waveform", overview);
                if (overview_response.clicked() || overview_response.dragged())
                    && let Some(pointer) = overview_response.interact_pointer_pos()
                {
                    self.wave_drag_start = None;
                    let seconds = f64::from(
                        ((pointer.x - overview.left()) / overview.width()).clamp(0.0, 1.0),
                    ) * duration;
                    self.position = self.inspect_seek_position(seconds, duration);
                    self.preview
                        .seek(self.position, overview_response.dragged());
                }
                if overview_response.drag_stopped() {
                    self.preview.seek(self.position, false);
                }
                let (rect, response) = ui.allocate_exact_size(
                    vec2(ui.available_width(), detail_height),
                    Sense::click_and_drag(),
                );
                self.hit("Detail waveform", rect);
                if (response.clicked() || response.dragged())
                    && let Some(pointer) = response.interact_pointer_pos()
                {
                    let window = self.waveform_window(&track);
                    if response.drag_started() {
                        // Keep the displayed range fixed under the finger until release.
                        self.wave_drag_start = Some(window.start);
                    }
                    let seconds = (window.start
                        + f64::from(((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0))
                            * (window.end - window.start))
                        .clamp(0.0, duration);
                    self.position = self.inspect_seek_position(seconds, duration);
                    self.preview.seek(self.position, response.dragged());
                }
                if response.drag_stopped() {
                    self.wave_drag_start = None;
                    self.preview.seek(self.position, false);
                }
                let window = self.waveform_window(&track);
                if phrase_height > 0.0 {
                    self.hit(
                        "Full phrases",
                        overview.with_min_y(overview.bottom() - phrase_height),
                    );
                    self.hit(
                        "Detail phrases",
                        rect.with_min_y(rect.bottom() - phrase_height),
                    );
                }
                self.waveform(
                    ui.painter(),
                    overview,
                    0.0..duration.max(1.0),
                    "FULL TRACK",
                    Some(window.clone()),
                );
                let grid_available = self
                    .detail
                    .as_ref()
                    .is_some_and(|d| !d.beat_grid.is_empty());
                self.waveform(
                    ui.painter(),
                    rect,
                    window.clone(),
                    &format!(
                        "DETAIL · {:.0}×{}",
                        self.zoom,
                        if self.detail.is_some() && !grid_available {
                            " · No beat grid"
                        } else {
                            ""
                        }
                    ),
                    None,
                );
                self.paint_inspect_cursor(ui.painter(), overview, 0.0..duration.max(1.0), false);
                self.paint_inspect_cursor(ui.painter(), rect, window, true);
                self.hot_cue_pads(ui, duration);
                if self.loop_controls {
                    self.loop_toolbar(ui, duration);
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} / {}", time(self.position), time(duration)))
                                .monospace()
                                .size(14.0),
                        );
                        self.beat_jump_controls(ui, duration);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let zoom_in = theme::button(ui, "+", vec2(48.0, 48.0), false);
                            self.hit("Zoom in", zoom_in.rect);
                            if zoom_in.clicked() {
                                self.set_zoom((self.zoom * 2.0).min(256.0));
                            }
                            let zoom_out = theme::button(ui, "−", vec2(48.0, 48.0), false);
                            self.hit("Zoom out", zoom_out.rect);
                            if zoom_out.clicked() {
                                self.set_zoom((self.zoom / 2.0).max(1.0));
                            }
                            let fit = theme::button(ui, "Fit", vec2(56.0, 48.0), false);
                            self.hit("Fit waveform", fit.rect);
                            if fit.clicked() {
                                self.set_zoom(1.0);
                            }
                            let grid = ui
                                .add_enabled_ui(grid_available, |ui| {
                                    theme::button(ui, "Grid", vec2(72.0, 48.0), self.show_grid)
                                })
                                .inner
                                .on_hover_text(if grid_available {
                                    "Show beat lines and highlight each bar's first beat"
                                } else {
                                    "No beat grid available for this track"
                                });
                            self.hit("Grid", grid.rect);
                            if grid.clicked() {
                                self.show_grid = !self.show_grid;
                            }
                            self.quantize_button(ui, duration);
                        });
                    });
                }
                let memory_cues: Vec<_> = self
                    .detail
                    .as_ref()
                    .map(|d| d.cues.iter().filter(|c| c.number > 8).cloned().collect())
                    .unwrap_or_default();
                if !memory_cues.is_empty() {
                    ui.horizontal_wrapped(|ui| {
                        for cue in memory_cues {
                            let slot = crate::cues::slot(cue.number);
                            let label = format!(
                                "{}  {}{}",
                                slot,
                                time(f64::from(cue.time_ms) / 1000.0),
                                if cue.kind == "loop"
                                    && i64::from(cue.loop_ms) > i64::from(cue.time_ms)
                                {
                                    " ↻"
                                } else {
                                    ""
                                }
                            );
                            let (pad, response) =
                                ui.allocate_exact_size(vec2(122.0, 48.0), Sense::click());
                            let color = crate::cues::color(cue.color_id);
                            ui.painter().rect_filled(pad, 8, color.gamma_multiply(0.13));
                            theme::text(
                                ui.painter(),
                                Rect::from_min_size(
                                    pad.min + vec2(9.0, 4.0),
                                    vec2(
                                        pad.width() - 18.0,
                                        if cue.label.is_empty() { 40.0 } else { 22.0 },
                                    ),
                                ),
                                label,
                                14.0,
                                color,
                            );
                            if !cue.label.is_empty() {
                                theme::text(
                                    ui.painter(),
                                    Rect::from_min_size(
                                        pad.min + vec2(9.0, 27.0),
                                        vec2(pad.width() - 18.0, 16.0),
                                    ),
                                    &cue.label,
                                    11.0,
                                    TEXT,
                                );
                            }
                            self.hit(&format!("Cue {}", cue.number), response.rect);
                            if response.clicked() {
                                self.position = (f64::from(cue.time_ms) / 1000.0).min(duration);
                                self.preview.seek(self.position, false);
                            }
                        }
                    });
                }
                if let Some(error) = &self.preview.state.error {
                    ui.colored_label(ORANGE, error);
                }
                if let Some(detail) = &self.detail
                    && !detail.warning.is_empty()
                {
                    ui.colored_label(ORANGE, &detail.warning);
                }
            });
    }

    fn inspect_actions(&mut self, ui: &mut Ui, track: &Track) {
        let duration = self.preview_duration(track);
        ui.horizontal(|ui| {
            let response = ui
                .add_enabled_ui(
                    self.fresh() && !self.load_busy && self.pending_load.is_none(),
                    |ui| theme::primary(ui, "Load to player…", vec2(184.0, 48.0)),
                )
                .inner;
            self.hit("Inspect load", response.rect);
            if response.clicked() {
                self.load_dialog = true;
            }
            let playing = self.preview.request.playing
                && !matches!(self.preview.state.phase, Phase::Error | Phase::Ended);
            let play = ui
                .add_enabled_ui(
                    self.options.demo || (!track.file_missing && self.library_session.is_some()),
                    |ui| {
                        theme::button(
                            ui,
                            if playing { "Pause" } else { "Play" },
                            vec2(110.0, 48.0),
                            playing,
                        )
                    },
                )
                .inner;
            self.hit("Preview play", play.rect);
            if play.clicked() {
                self.preview.toggle(self.position);
                if self.preview.state.phase == Phase::Ended {
                    self.position = 0.0;
                }
            }
            let edit = ui
                .add_enabled_ui(self.can_edit_cues() && duration > 0.0, |ui| {
                    theme::button(ui, "Edit cues…", vec2(128.0, 48.0), false)
                })
                .inner;
            self.hit("Edit cues", edit.rect);
            if edit.clicked() {
                self.open_cue_editor();
            }
            let loops = theme::button(ui, "Loop", vec2(110.0, 48.0), self.loop_controls);
            self.hit("Loop controls", loops.rect);
            if loops.clicked() {
                self.loop_controls = !self.loop_controls;
            }
        });
    }

    fn waveform_window(&self, track: &Track) -> std::ops::Range<f64> {
        let duration = self.preview_duration(track).max(1.0);
        let span = duration / self.zoom;
        let start = self
            .wave_drag_start
            .unwrap_or(self.position - span / 2.0)
            .clamp(0.0, (duration - span).max(0.0));
        start..start + span
    }

    fn preview_duration(&self, track: &Track) -> f64 {
        if self.preview.state.epoch == self.preview.request.epoch
            && self.preview.state.duration > 0.0
        {
            self.preview.state.duration
        } else {
            f64::from(track.duration)
        }
    }

    fn scan_audio(&mut self, ctx: &Context) {
        if self.audio_scan.is_some() {
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.audio_scan = Some(receiver);
        self.audio_scan_error.clear();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let _ = sender.send(preview::devices());
            ctx.request_repaint();
        });
    }

    fn save_audio(&mut self) {
        self.preview.submit();
        if let Some(path) = &self.settings_path
            && let Err(error) = settings::save_audio(
                path,
                self.preview.request.device.clone(),
                self.preview.request.volume,
            )
        {
            self.notice = format!("Could not save audio settings: {error}");
        }
    }

    fn audio_modal(&mut self, ctx: &Context) {
        Modal::new(Id::unique("audio-output")).show(ctx, |ui| {
            ui.set_width((ctx.content_rect().width() - 64.0).min(560.0));
            ui.heading("Preview output");
            ui.label(
                RichText::new("Choose an output for local track previews.")
                    .size(14.0)
                    .color(MUTED),
            );
            let selected = self
                .preview
                .request
                .device
                .as_ref()
                .map_or("System default", |d| d.name.as_str());
            ui.add(Label::new(format!("Selected: {selected}")).truncate());
            ScrollArea::vertical()
                .id_salt("audio-devices")
                .max_height((ctx.content_rect().height() - 250.0).max(80.0))
                .scroll_source(scroll_area::ScrollSource::ALL)
                .show(ui, |ui| {
                    if theme::button(
                        ui,
                        "System default",
                        vec2(ui.available_width(), 48.0),
                        self.preview.request.device.is_none(),
                    )
                    .clicked()
                    {
                        self.preview.request.device = None;
                        self.save_audio();
                    }
                    for device in self.audio_devices.clone() {
                        let label = format!("{} · {}", device.name, device.host);
                        if ui
                            .add_sized(
                                vec2(ui.available_width(), 48.0),
                                Button::new(RichText::new(label).size(15.0))
                                    .selected(self.preview.request.device.as_ref() == Some(&device))
                                    .truncate(),
                            )
                            .clicked()
                        {
                            self.preview.request.device = Some(device);
                            self.save_audio();
                        }
                    }
                    if self.audio_scan.is_some() {
                        ui.label("Finding audio outputs…");
                    }
                    if !self.audio_scan_error.is_empty() {
                        ui.colored_label(ORANGE, &self.audio_scan_error);
                    }
                });
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Volume {:.0}%",
                    self.preview.request.volume * 100.0
                ));
                for (label, delta) in [("−", -0.1), ("+", 0.1)] {
                    let button = theme::button(ui, label, vec2(48.0, 48.0), false);
                    self.hit(&format!("Volume {label}"), button.rect);
                    if button.clicked() {
                        self.preview.request.volume =
                            (self.preview.request.volume + delta).clamp(0.0, 1.0);
                        self.save_audio();
                    }
                }
                if theme::button(ui, "Refresh", vec2(94.0, 48.0), false).clicked() {
                    self.scan_audio(ctx);
                }
                let close = theme::primary(ui, "Done", vec2(90.0, 48.0));
                self.hit("Audio done", close.rect);
                if close.clicked() {
                    self.audio_dialog = false;
                }
            });
        });
    }

    fn inspect_phrase_height(&self) -> f32 {
        if self
            .detail
            .as_ref()
            .is_some_and(|detail| !detail.phrases.is_empty())
        {
            crate::phrases::STRIP_HEIGHT
        } else {
            0.0
        }
    }

    fn waveform(
        &self,
        p: &Painter,
        rect: Rect,
        range: std::ops::Range<f64>,
        label: &str,
        window: Option<std::ops::Range<f64>>,
    ) {
        let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
        p.rect_filled(rect, 8, PANEL);
        p.text(
            rect.left_top() + vec2(7.0, 3.0),
            Align2::LEFT_TOP,
            label,
            FontId::proportional(10.0),
            MUTED,
        );
        p.text(
            rect.right_top() + vec2(-7.0, 3.0),
            Align2::RIGHT_TOP,
            format!("{} – {}", time(range.start), time(range.end)),
            FontId::monospace(10.0),
            MUTED,
        );
        let phrase_height = self.inspect_phrase_height();
        if phrase_height > 0.0
            && let Some(detail) = &self.detail
        {
            detail.phrases.paint(
                &p,
                rect.with_min_y(rect.bottom() - phrase_height),
                range.clone(),
            );
        }
        let rect = Rect::from_min_max(
            rect.min + vec2(0.0, 17.0),
            rect.max - vec2(0.0, phrase_height + 3.0),
        );
        let Some(detail) = &self.detail else {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                if self.detail_busy {
                    "Preparing waveform…"
                } else {
                    "Waveform unavailable"
                },
                FontId::proportional(13.0),
                MUTED,
            );
            return;
        };
        let wave = &detail.waveform;
        let cue_edge = if window.is_some() {
            crate::waveform::markers::Edge::Top
        } else {
            crate::waveform::markers::Edge::Bottom
        };
        let use_bands = self.waveform_style.uses_bands(detail.bands.available());
        if wave.entries.is_empty() && !use_bands {
            p.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "No waveform available",
                FontId::proportional(13.0),
                MUTED,
            );
            crate::waveform::markers::hot_cues(&p, rect, range, &detail.cues, cue_edge);
            return;
        }
        let start = range.start;
        let span = range.end - start;
        let rate = f64::from(wave.sample_rate);
        if window.is_none() {
            crate::waveform::detail::paint(
                &p,
                rect.shrink2(vec2(0.0, 2.0)),
                self.waveform_style,
                &wave.entries,
                wave.sample_rate,
                &detail.bands,
                range.clone(),
            );
        } else if use_bands {
            crate::waveform::half_bands(
                &p,
                rect.shrink2(vec2(0.0, 2.0)),
                &detail.bands,
                range.clone(),
            );
        } else {
            // Full-track overviews retain peak reduction at physical-pixel resolution.
            let bins = (rect.width() * p.ctx().pixels_per_point())
                .ceil()
                .clamp(1.0, 4096.0) as usize;
            for bin in 0..bins {
                let begin = start + bin as f64 / bins as f64 * span;
                let end = start + (bin + 1) as f64 / bins as f64 * span;
                let a = (begin * rate) as usize;
                let b = (end * rate).ceil() as usize;
                let spectral = wave
                    .entries
                    .get(a.min(wave.entries.len())..b.min(wave.entries.len()))
                    .and_then(|entries| entries.iter().max_by_key(|e| e.h))
                    .copied();
                crate::waveform::column(
                    &p,
                    Rect::from_min_max(
                        pos2(
                            rect.left() + bin as f32 * rect.width() / bins as f32,
                            rect.top() + 2.0,
                        ),
                        pos2(
                            rect.left() + (bin + 1) as f32 * rect.width() / bins as f32,
                            rect.bottom() - 2.0,
                        ),
                    ),
                    self.waveform_style,
                    if self.waveform_style == crate::waveform::Style::ThreeBand {
                        crate::waveform::Overview::Half
                    } else {
                        self.waveform_overview
                    },
                    spectral,
                    None,
                );
            }
        }

        if let Some(window) = window {
            let viewport = Rect::from_x_y_ranges(
                rect.left() + ((window.start - start) / span) as f32 * rect.width()
                    ..=rect.left() + ((window.end - start) / span) as f32 * rect.width(),
                rect.y_range(),
            );
            p.rect_filled(viewport, 2, ORANGE.gamma_multiply(0.18));
            p.rect_stroke(viewport, 2, Stroke::new(1.5, ORANGE), StrokeKind::Inside);
        } else if self.show_grid {
            // Leave room between lines as we zoom out: beats, then bars,
            // then every 2/4/8 bars, aligned to the saved grid while scrolling.
            let budget = (rect.width() / 32.0).floor().max(1.0) as usize;
            let beats = detail.beat_grid.visible(start, range.end, budget);
            let label_all = rect.width() / beats.clone().count().max(1) as f32 >= 28.0;
            let mut label_right = rect.left();
            for beat in beats {
                let x = rect.left() + ((beat.seconds() - start) / span) as f32 * rect.width();
                let downbeat = beat.beat_in_bar == 1;
                let color = if downbeat {
                    Color32::from_rgb(237, 105, 121)
                } else {
                    Color32::from_white_alpha(72)
                };
                p.vline(
                    x,
                    rect.y_range(),
                    Stroke::new(if downbeat { 1.5 } else { 1.0 }, color),
                );
                if (label_all || downbeat) && x >= label_right && x + 14.0 <= rect.right() {
                    let label_rect =
                        Rect::from_min_size(pos2(x + 2.0, rect.top()), vec2(10.0, 12.0));
                    p.rect_filled(label_rect, 2, PANEL);
                    p.text(
                        label_rect.center(),
                        Align2::CENTER_CENTER,
                        beat.beat_in_bar.to_string(),
                        FontId::monospace(10.0),
                        if downbeat { color } else { MUTED },
                    );
                    label_right = x + 24.0;
                }
            }
        }
        for cue in detail
            .cues
            .iter()
            .filter(|cue| !(1..=8).contains(&cue.number))
        {
            let phase = (f64::from(cue.time_ms) / 1000.0 - start) / span;
            if (0.0..=1.0).contains(&phase) {
                let x = rect.left() + phase as f32 * rect.width();
                p.line_segment(
                    [pos2(x, rect.top() + 5.0), pos2(x, rect.bottom() - 5.0)],
                    Stroke::new(1.0, crate::cues::color(cue.color_id)),
                );
            }
        }
        if let Some(region) = self.preview.request.loop_region {
            self.paint_loop(&p, rect, range.clone(), region);
        }
        crate::waveform::markers::hot_cues(&p, rect, range, &detail.cues, cue_edge);
    }

    fn keyboard(&mut self, root: &mut Ui) {
        let Some(field) = self.keyboard else {
            return;
        };
        let mut changed = false;
        Panel::bottom("keyboard")
            .exact_size(232.0)
            .frame(Frame::new().fill(PANEL).inner_margin(8))
            .show(root, |ui| {
                ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
                let rows = if self.symbols {
                    ["1234567890", "-/:.=_@?&", "+#%~[]()"]
                } else {
                    ["qwertyuiop", "asdfghjkl", "zxcvbnm"]
                };
                let key_width = (ui.available_width() - 128.0 - 50.0) / 10.0;
                for (row_index, row) in rows.into_iter().enumerate() {
                    ui.horizontal(|ui| {
                        if row_index > 0 {
                            ui.add_space(
                                (ui.available_width() - row.len() as f32 * (key_width + 5.0) + 5.0)
                                    / 2.0,
                            );
                        }
                        for key in row.chars() {
                            let response =
                                theme::button(ui, &key.to_string(), vec2(key_width, 44.0), false);
                            self.hit(&format!("Key {key}"), response.rect);
                            if response.clicked() {
                                let value = if field == Field::Search {
                                    &mut self.query
                                } else {
                                    &mut self.server_draft
                                };
                                value.push(key);
                                changed = true;
                            }
                        }
                        if row_index == 0 {
                            let backspace =
                                theme::button(ui, "Backspace", vec2(128.0, 44.0), false);
                            self.hit("Keyboard backspace", backspace.rect);
                            if backspace.clicked() {
                                if field == Field::Search {
                                    self.query.pop();
                                } else {
                                    self.server_draft.pop();
                                }
                                changed = true;
                            }
                        }
                    });
                }
                ui.horizontal(|ui| {
                    if theme::button(
                        ui,
                        if self.symbols { "ABC" } else { "123 / :" },
                        vec2(key_width * 1.5, 44.0),
                        false,
                    )
                    .clicked()
                    {
                        self.symbols = !self.symbols;
                    }
                    if theme::button(ui, "Clear", vec2(key_width * 1.5, 44.0), false).clicked() {
                        if field == Field::Search {
                            self.query.clear();
                        } else {
                            self.server_draft.clear();
                        }
                        changed = true;
                    }
                    let done_width = key_width * 2.5;
                    let space_width =
                        (ui.available_width() - done_width - ui.spacing().item_spacing.x).max(0.0);
                    if theme::button(ui, "Space", vec2(space_width, 44.0), false).clicked() {
                        if field == Field::Search {
                            self.query.push(' ');
                        } else {
                            self.server_draft.push(' ');
                        }
                        changed = true;
                    }
                    let done = theme::primary(ui, "Done", vec2(done_width, 44.0));
                    self.hit("Keyboard done", done.rect);
                    if done.clicked() {
                        self.keyboard = None;
                    }
                });
            });
        if changed && field == Field::Search {
            self.filter();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, root: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = root.ctx().clone();
        self.frames += 1;
        self.render(root);
        if self.options.screenshot.is_some() || self.options.smoke_test {
            ctx.request_repaint_after(Duration::from_millis(80));
            if self.frames >= 8 && !self.screenshot_requested {
                if let Some(path) = self.options.screenshot.clone() {
                    self.screenshot_requested = true;
                    let callback_ctx = ctx.clone();
                    ctx.request_screenshot(move |image| {
                        let pixels: Vec<u8> =
                            image.pixels.iter().flat_map(|p| p.to_array()).collect();
                        if let Err(error) = image::save_buffer(
                            &path,
                            &pixels,
                            image.width() as u32,
                            image.height() as u32,
                            image::ColorType::Rgba8,
                        ) {
                            eprintln!("Screenshot failed: {error}");
                            std::process::exit(1);
                        }
                        println!("Screenshot saved to {path}");
                        callback_ctx.send_viewport_cmd(ViewportCommand::Close);
                    });
                } else {
                    ctx.send_viewport_cmd(ViewportCommand::Close);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Existing card-specific flows choose Cards explicitly; new installations
    // now start with the waveform view.
    pub(super) fn cards_app(
        ctx: &Context,
        options: Options,
        settings_path: Option<PathBuf>,
    ) -> App {
        let mut app = App::new(ctx, options, settings_path);
        app.deck_compare = false;
        app
    }

    #[test]
    fn deck_cards_fit_and_guard_loads_and_external_inspection() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            let ctx = Context::default();
            let mut app = cards_app(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            app.selected = Some(2);
            app.page = Page::Players;
            frame(&mut app, &ctx, size, vec![]);
            let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
            for label in [
                "Deck load 1",
                "Deck load 2",
                "Deck inspect 1",
                "Deck inspect 2",
                "Deck waveform 1",
                "Deck waveform 2",
            ] {
                let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
                assert!(
                    bounds.contains_rect(rect),
                    "{label} outside content: {rect:?}"
                );
            }
            click(&mut app, &ctx, size, "Deck load 1");
            assert_eq!(
                app.status.players[0].track_id, 1,
                "playing deck must wait for confirmation"
            );
            assert!(app.load_confirmation.is_some());
            click(&mut app, &ctx, size, "Load cancel");
            click(&mut app, &ctx, size, "Deck load 2");
            assert_eq!(app.status.players[1].track_id, 2);
            click(&mut app, &ctx, size, "Deck inspect 2");
            assert_eq!(app.detail_track.as_ref().unwrap().id, 2);
            app.page = Page::Players;
            app.status.players[1].external = true;
            app.status.players[1].source = "USB · player 3".into();
            click(&mut app, &ctx, size, "Deck inspect 2");
            assert_eq!(
                app.page,
                Page::Players,
                "external ID must never open a same-ID library track"
            );
            app.linked = false;
            click(&mut app, &ctx, size, "Deck load 2");
            assert!(
                app.status.players[1].external,
                "link-off load should be blocked"
            );
            click(&mut app, &ctx, size, "Deck pair");
            click(&mut app, &ctx, size, "Pair 0 player 3");
            click(&mut app, &ctx, size, "Pair 1 player 4");
            click(&mut app, &ctx, size, "Pair apply");
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.hits.iter().any(|(n, _)| n == "Deck load 4"));
        }
    }

    #[test]
    fn cue_editor_touch_save_cancel_label_color_and_delete_survive_reopening() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            let ctx = Context::default();
            let mut app = cards_app(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            let (controller, _requests, _results) = Controller::test();
            app.preview = controller;
            app.inspect(app.tracks[0].clone());
            app.position = 32.18;
            click(&mut app, &ctx, size, "Edit cues");
            assert_eq!(app.cue_editor.as_ref().unwrap().draft.number, 5);
            assert_eq!(app.cue_editor.as_ref().unwrap().draft.time_ms, 32000);
            click(&mut app, &ctx, size, "Cue nudge +");
            assert_eq!(app.cue_editor.as_ref().unwrap().draft.time_ms, 32010);
            click(&mut app, &ctx, size, "Cue color 42");
            click(&mut app, &ctx, size, "Cue label");
            click(&mut app, &ctx, size, "Cue key a");
            click(&mut app, &ctx, size, "Cue label done");
            click(&mut app, &ctx, size, "Cue cancel");
            assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
            click(&mut app, &ctx, size, "Edit cues");
            click(&mut app, &ctx, size, "Cue color 42");
            click(&mut app, &ctx, size, "Cue label");
            click(&mut app, &ctx, size, "Cue key a");
            click(&mut app, &ctx, size, "Cue label done");
            click(&mut app, &ctx, size, "Cue save");
            assert!(app.cue_editor.is_none());
            let saved = app
                .detail
                .as_ref()
                .unwrap()
                .cues
                .iter()
                .find(|c| c.number == 5)
                .unwrap()
                .clone();
            assert_eq!(saved.color_id, 42);
            assert_eq!(saved.label, "a");
            app.inspect(app.tracks[1].clone());
            app.inspect(app.tracks[0].clone());
            assert!(app.detail.as_ref().unwrap().cues.contains(&saved));
            click(&mut app, &ctx, size, "Edit cues");
            click(&mut app, &ctx, size, "Edit slot 5");
            click(&mut app, &ctx, size, "Cue delete");
            assert!(
                !app.detail
                    .as_ref()
                    .unwrap()
                    .cues
                    .iter()
                    .any(|c| c.number == 5)
            );
        }
    }

    #[test]
    fn preview_controls_seek_without_autoplay_and_stop_when_leaving_inspect() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            let ctx = Context::default();
            let mut app = cards_app(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            let (controller, requests, results) = Controller::test();
            app.preview = controller;
            app.inspect(app.tracks[0].clone());
            click(&mut app, &ctx, size, "Cue 2");
            assert_eq!(requests.borrow().seconds, 32.0);
            assert!(!requests.borrow().playing);
            click(&mut app, &ctx, size, "Preview play");
            assert!(requests.borrow().playing);
            assert_eq!(requests.borrow().seconds, 32.0);
            click(&mut app, &ctx, size, "Full waveform");
            assert!((requests.borrow().seconds - 142.0).abs() < 0.1);
            assert!(requests.borrow().playing);
            let request = requests.borrow().clone();
            results
                .send(preview::Snapshot {
                    epoch: request.epoch,
                    revision: request.revision,
                    phase: Phase::Playing,
                    position: 150.0,
                    ..Default::default()
                })
                .unwrap();
            frame(&mut app, &ctx, size, vec![]);
            assert_eq!(app.position, 150.0);
            click(&mut app, &ctx, size, "Preview play");
            assert!(!requests.borrow().playing);
            click(&mut app, &ctx, size, "Back");
            assert!(requests.borrow().track.is_none());
            app.inspect(app.tracks[0].clone());
            click(&mut app, &ctx, size, "Preview play");
            assert!(!app.hits.iter().any(|(name, _)| name == "Preview output"));
            click(&mut app, &ctx, size, "Setup");
            assert!(requests.borrow().track.is_none());
            assert!(!requests.borrow().playing);
            frame(
                &mut app,
                &ctx,
                size,
                vec![
                    eframe::egui::Event::PointerMoved(pos2(600.0, 350.0)),
                    eframe::egui::Event::MouseWheel {
                        unit: MouseWheelUnit::Point,
                        delta: vec2(0.0, -600.0),
                        phase: TouchPhase::Move,
                        source: MouseWheelSource::Unknown,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
            for _ in 0..20 {
                frame(&mut app, &ctx, size, vec![]);
            }
            // Keep enumeration pending so this UI check needs no audio hardware.
            let (_scan_sender, scan_receiver) = std::sync::mpsc::sync_channel(1);
            app.audio_scan = Some(scan_receiver);
            click(&mut app, &ctx, size, "Preview output");
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.audio_dialog, "the chooser must stay open on Settings");
            let volume = app.preview.request.volume;
            click(&mut app, &ctx, size, "Volume −");
            assert!((requests.borrow().volume - (volume - 0.1)).abs() < 0.001);
            assert!(requests.borrow().track.is_none());
            click(&mut app, &ctx, size, "Audio done");
            assert!(!app.audio_dialog);
            assert!(app.page == Page::Settings);
        }
    }

    pub(super) fn frame(
        app: &mut App,
        ctx: &Context,
        size: Vec2,
        events: Vec<eframe::egui::Event>,
    ) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| app.render(ui));
        output.textures_delta.clear();
    }

    pub(super) fn click(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
        frame(app, ctx, size, vec![]);
        let rect = app
            .hits
            .iter()
            .find(|(s, _)| s == label)
            .unwrap_or_else(|| panic!("Missing {label}: {:?}", app.hits))
            .1;
        assert!(
            Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
            "{label} outside screen: {rect:?}"
        );
        let pos = rect.center();
        frame(app, ctx, size, vec![eframe::egui::Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            frame(
                app,
                ctx,
                size,
                vec![
                    eframe::egui::Event::PointerMoved(pos),
                    eframe::egui::Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    },
                ],
            );
        }
    }

    #[test]
    fn touch_flow_fits_small_and_large_landscape_screens() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            let ctx = Context::default();
            let mut app = cards_app(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            frame(&mut app, &ctx, size, vec![]);
            click(&mut app, &ctx, size, "Track 1");
            assert_eq!(app.selected, Some(1));
            click(&mut app, &ctx, size, "Inspect");
            assert_eq!(app.page, Page::Detail);
            assert!(!app.detail.as_ref().unwrap().waveform.entries.is_empty());
            click(&mut app, &ctx, size, "Back");
            click(&mut app, &ctx, size, "Playlists");
            click(&mut app, &ctx, size, "Warm-up selections");
            assert_eq!(app.source, Some(1));
            assert_eq!(app.visible.len(), 24);
            click(&mut app, &ctx, size, "Players");
            assert_eq!(app.page, Page::Players);
            click(&mut app, &ctx, size, "Collection");
            click(&mut app, &ctx, size, "Search");
            assert!(app.keyboard == Some(Field::Search));
            frame(&mut app, &ctx, size, vec![]);
            click(&mut app, &ctx, size, "Key a");
            assert_eq!(app.query, "a");
            click(&mut app, &ctx, size, "Keyboard done");
            assert!(app.keyboard.is_none());
            // Exercise the longer live connection label, search-clear button,
            // and Settings shortcut together on the smallest touch display.
            app.options.demo = false;
            app.last_status = Some(Instant::now());
            app.connection = api::Connection::Connected;
            click(&mut app, &ctx, size, "Search");
            frame(&mut app, &ctx, size, vec![]);
            let header = Rect::from_min_size(Pos2::ZERO, vec2(size.x, 52.0));
            let mut right = 0.0;
            for label in [
                "Search",
                "Clear search",
                "Library filters",
                "Library sort",
                "Player count",
                "Connection status",
                "Setup",
            ] {
                let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                assert!(
                    header.contains_rect(rect),
                    "{label} outside header: {rect:?}"
                );
                assert!(rect.left() >= right, "{label} overlaps previous control");
                right = rect.right();
            }
            click(&mut app, &ctx, size, "Setup");
            assert_eq!(app.page, Page::Settings);
            assert!(app.keyboard.is_none());
            click(&mut app, &ctx, size, "Collection");
            assert_eq!(app.source, Some(1));
            assert_eq!(app.query, "a");
        }
    }

    #[test]
    fn collection_touchdown_at_a_new_position_selects_without_scrolling() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            for touch in [false, true] {
                let ctx = Context::default();
                let mut app = cards_app(
                    &ctx,
                    Options {
                        demo: true,
                        ..Default::default()
                    },
                    None,
                );
                frame(&mut app, &ctx, size, vec![]);
                frame(&mut app, &ctx, size, vec![]);
                let first = app.hits.iter().find(|(n, _)| n == "Track 1").unwrap().1;
                let pos = first.center();
                // A touch monitor relocates the pointer and presses in the same frame.
                // There is no hover at the new location before the finger lands.
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![eframe::egui::Event::PointerMoved(pos + vec2(0.0, 132.0))],
                );
                for pressed in [true, false] {
                    let mut events = vec![
                        eframe::egui::Event::PointerMoved(pos),
                        eframe::egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Modifiers::NONE,
                        },
                    ];
                    if touch {
                        events.push(eframe::egui::Event::Touch {
                            device_id: TouchDeviceId(1),
                            id: TouchId(1),
                            phase: if pressed {
                                TouchPhase::Start
                            } else {
                                TouchPhase::End
                            },
                            pos,
                            force: None,
                        });
                        if !pressed {
                            events.push(eframe::egui::Event::PointerGone);
                        }
                    }
                    frame(&mut app, &ctx, size, events);
                }
                for _ in 0..20 {
                    frame(&mut app, &ctx, size, vec![]);
                }
                let after = app
                    .hits
                    .iter()
                    .find(|(n, _)| n == "Track 1")
                    .map(|(_, r)| *r);
                assert_eq!(
                    after,
                    Some(first),
                    "touchdown scrolled the list: touch={touch}, size={size:?}"
                );
                assert_eq!(app.selected, Some(1));
                assert_eq!(app.page, Page::Collection);
            }
        }
    }

    #[test]
    fn collection_drag_scrolls_without_selecting_and_taps_still_select() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
            for touch in [false, true] {
                let ctx = Context::default();
                let mut app = cards_app(
                    &ctx,
                    Options {
                        demo: true,
                        ..Default::default()
                    },
                    None,
                );
                click(&mut app, &ctx, size, "Track 1");
                let start = app
                    .hits
                    .iter()
                    .find(|(s, _)| s == "Track 3")
                    .unwrap()
                    .1
                    .center();
                let input = |pos, phase| {
                    let mut events = vec![eframe::egui::Event::PointerMoved(pos)];
                    if phase != TouchPhase::Move {
                        events.push(eframe::egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: phase == TouchPhase::Start,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    if touch {
                        events.push(eframe::egui::Event::Touch {
                            device_id: TouchDeviceId(1),
                            id: TouchId(1),
                            phase,
                            pos,
                            force: None,
                        });
                    }
                    events
                };
                frame(&mut app, &ctx, size, input(start, TouchPhase::Start));
                for step in 1..=6 {
                    frame(
                        &mut app,
                        &ctx,
                        size,
                        input(start - vec2(0.0, step as f32 * 20.0), TouchPhase::Move),
                    );
                }
                // The virtualized list must advance beyond its first row.
                frame(&mut app, &ctx, size, vec![]);
                assert!(
                    !app.hits.iter().any(|(s, _)| s == "Track 1"),
                    "drag did not scroll the collection: touch={touch}, size={size:?}"
                );
                let fourth = app.hits.iter().find(|(n, _)| n == "Track 4").unwrap().1;
                assert!(
                    (fourth.top() - (start.y + fourth.height() / 2.0 - 120.0)).abs() < 1.0,
                    "drag must move exactly 120 points, excluding pre-contact movement"
                );
                let end = start - vec2(0.0, 120.0);
                // Stop moving before release so this check doesn't depend on inertia.
                for _ in 0..20 {
                    frame(&mut app, &ctx, size, vec![]);
                }
                frame(&mut app, &ctx, size, input(end, TouchPhase::End));
                assert_eq!(app.selected, Some(1), "drag must not select another track");
                assert_eq!(app.page, Page::Collection, "drag must not open Inspect");
                click(&mut app, &ctx, size, "Track 4");
                assert_eq!(app.selected, Some(4), "tap after dragging must select");
            }
        }
    }

    #[test]
    fn overview_and_detail_stay_in_sync_with_touch_and_pointer() {
        for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
            for touch in [false, true] {
                let ctx = Context::default();
                let mut app = cards_app(
                    &ctx,
                    Options {
                        demo: true,
                        ..Default::default()
                    },
                    None,
                );
                // This interaction test sets the cursor directly; an asynchronous
                // preview reply could otherwise restore the previous seek target.
                let (controller, _requests, _results) = Controller::test();
                app.preview = controller;
                let track = app.tracks[0].clone();
                let duration = f64::from(track.duration);
                app.inspect(track.clone());
                frame(&mut app, &ctx, size, vec![]);
                frame(&mut app, &ctx, size, vec![]);
                let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
                for label in [
                    "Full waveform",
                    "Detail waveform",
                    "Zoom in",
                    "Cue 4",
                    "Inspect load",
                ] {
                    let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                    assert!(
                        bounds.contains_rect(rect),
                        "{label} outside content: {rect:?}"
                    );
                    assert!(rect.height() >= 48.0);
                }
                assert_eq!(app.zoom, 8.0);
                assert_eq!(app.waveform_window(&track), 0.0..duration / 8.0);
                assert!(app.show_grid);
                click(&mut app, &ctx, size, "Grid");
                assert!(!app.show_grid);
                click(&mut app, &ctx, size, "Grid");
                assert!(app.show_grid);
                let overview = app
                    .hits
                    .iter()
                    .find(|(name, _)| name == "Full waveform")
                    .unwrap()
                    .1;
                for name in [
                    "Back",
                    "Inspect artwork",
                    "Inspect title",
                    "Inspect artist",
                    "Inspect BPM",
                    "Inspect key",
                    "Edit track",
                    "Inspect playlists",
                ] {
                    let rect = app.hits.iter().find(|(label, _)| label == name).unwrap().1;
                    assert!(
                        bounds.contains_rect(rect),
                        "{name} outside screen at {size:?}"
                    );
                    assert!(
                        rect.bottom() < overview.top(),
                        "{name} overlaps the waveform at {size:?}"
                    );
                }
                let input = |pos, phase| {
                    let mut events = vec![eframe::egui::Event::PointerMoved(pos)];
                    if phase != TouchPhase::Move {
                        events.push(eframe::egui::Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed: phase == TouchPhase::Start,
                            modifiers: Modifiers::NONE,
                        });
                    }
                    if touch {
                        events.push(eframe::egui::Event::Touch {
                            device_id: TouchDeviceId(1),
                            id: TouchId(1),
                            phase,
                            pos,
                            force: None,
                        });
                    }
                    events
                };
                let at = |rect: Rect, phase: f32| {
                    pos2(rect.left() + rect.width() * phase, rect.center().y)
                };
                let pos = at(overview, 0.75);
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![eframe::egui::Event::PointerMoved(pos)],
                );
                frame(&mut app, &ctx, size, input(pos, TouchPhase::Start));
                frame(&mut app, &ctx, size, input(pos, TouchPhase::End));
                assert!((app.position - duration * 0.75).abs() < 0.001);
                click(&mut app, &ctx, size, "Zoom in");
                assert_eq!(app.zoom, 16.0);
                assert!((app.position - duration * 0.75).abs() < 0.001);
                assert!(
                    (app.waveform_window(&track).end
                        - app.waveform_window(&track).start
                        - duration / 16.0)
                        .abs()
                        < 0.001
                );

                // The overview always maps to the full track, even when zoomed in.
                for (phase, expected) in [(1.1, duration), (-0.1, 0.0)] {
                    let start = at(overview, 0.5);
                    frame(
                        &mut app,
                        &ctx,
                        size,
                        vec![eframe::egui::Event::PointerMoved(start)],
                    );
                    frame(&mut app, &ctx, size, input(start, TouchPhase::Start));
                    let end = at(overview, phase);
                    frame(&mut app, &ctx, size, input(end, TouchPhase::Move));
                    assert_eq!(app.position, expected);
                    let window = app.waveform_window(&track);
                    assert!(window.start >= 0.0 && window.end <= duration);
                    frame(&mut app, &ctx, size, input(end, TouchPhase::End));
                }
                click(&mut app, &ctx, size, "Zoom out");
                assert_eq!(app.zoom, 8.0);
                app.position = duration * 0.5;
                let window = app.waveform_window(&track);
                frame(&mut app, &ctx, size, vec![]);
                let detail = app
                    .hits
                    .iter()
                    .find(|(name, _)| name == "Detail waveform")
                    .unwrap()
                    .1;
                let start = at(detail, 0.5);
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![eframe::egui::Event::PointerMoved(start)],
                );
                frame(&mut app, &ctx, size, input(start, TouchPhase::Start));
                for phase in [0.65, 0.8, 0.9] {
                    frame(
                        &mut app,
                        &ctx,
                        size,
                        input(at(detail, phase), TouchPhase::Move),
                    );
                    assert_eq!(
                        app.waveform_window(&track),
                        window,
                        "detail must stay still under a dragging finger"
                    );
                    assert!(
                        (app.position
                            - (window.start + f64::from(phase) * (window.end - window.start)))
                            .abs()
                            < 0.001
                    );
                }
                let position = app.position;
                frame(
                    &mut app,
                    &ctx,
                    size,
                    input(at(detail, 0.9), TouchPhase::End),
                );
                assert_eq!(app.position, position);
                assert!(app.wave_drag_start.is_none());
                click(&mut app, &ctx, size, "Cue 3");
                assert_eq!(app.position, 96.0);
                assert!(app.waveform_window(&track).contains(&96.0));
                click(&mut app, &ctx, size, "Fit waveform");
                assert_eq!(app.waveform_window(&track), 0.0..duration);
                assert_eq!(app.position, 96.0);
                for _ in 0..9 {
                    click(&mut app, &ctx, size, "Zoom in");
                }
                assert_eq!(app.zoom, 256.0);
                let window = app.waveform_window(&track);
                assert!((window.end - window.start - duration / 256.0).abs() < 0.001);
                assert_eq!(app.position, 96.0);
                click(&mut app, &ctx, size, "Fit waveform");
                click(&mut app, &ctx, size, "Zoom out");
                assert_eq!(app.zoom, 1.0);
            }
        }
    }

    #[test]
    fn waveform_zoom_survives_track_changes_and_restarts_including_fit() {
        let root = std::env::temp_dir().join(format!(
            "vynull-touch-zoom-ui-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("settings.json");
        let size = vec2(800.0, 480.0);
        let options = Options {
            demo: true,
            ..Default::default()
        };
        let ctx = Context::default();
        let mut app = cards_app(&ctx, options.clone(), Some(path.clone()));
        click(&mut app, &ctx, size, "Track 1");
        click(&mut app, &ctx, size, "Inspect");
        assert_eq!(app.zoom, 8.0);
        click(&mut app, &ctx, size, "Zoom in");
        click(&mut app, &ctx, size, "Zoom in");
        assert_eq!(app.zoom, 32.0);
        click(&mut app, &ctx, size, "Back");
        click(&mut app, &ctx, size, "Track 2");
        click(&mut app, &ctx, size, "Inspect");
        assert_eq!(app.zoom, 32.0);
        drop(app);

        let ctx = Context::default();
        let mut app = cards_app(&ctx, options.clone(), Some(path.clone()));
        app.inspect(app.tracks[2].clone());
        assert_eq!(app.zoom, 32.0);
        click(&mut app, &ctx, size, "Zoom out");
        assert_eq!(settings::load_zoom(Some(&path)), 16.0);
        click(&mut app, &ctx, size, "Fit waveform");
        drop(app);

        let ctx = Context::default();
        let mut app = cards_app(&ctx, options, Some(path.clone()));
        app.inspect(app.tracks[0].clone());
        assert_eq!(
            app.zoom, 1.0,
            "Fit should remain the default after a restart"
        );
        // A failed write must leave zoom usable in this session and report it.
        app.settings_path = Some(path.join("not-a-directory.json"));
        click(&mut app, &ctx, size, "Zoom in");
        assert_eq!(app.zoom, 2.0);
        assert!(app.notice.contains("could not save"));
        assert_eq!(settings::load_zoom(Some(&path)), 1.0);
        app.inspect(app.tracks[1].clone());
        assert_eq!(app.zoom, 2.0);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inspect_grid_thins_when_zoomed_out_and_restores_individual_beats() {
        let ctx = Context::default();
        let mut app = cards_app(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        let track = app.tracks[0].clone();
        app.inspect(track.clone());
        let paint = |app: &mut App, size| {
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|s| match s.shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == Color32::from_rgb(237, 105, 121)
                            || stroke.color == Color32::from_white_alpha(72) =>
                    {
                        Some((points, stroke.color == Color32::from_rgb(237, 105, 121)))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
            for zoom in [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 256.0] {
                app.zoom = zoom;
                for position in [0.0, 96.0, f64::from(track.duration)] {
                    app.position = position;
                    frame(&mut app, &ctx, size, vec![]);
                    let lines = paint(&mut app, size);
                    let rect = app
                        .hits
                        .iter()
                        .find(|(name, _)| name == "Detail waveform")
                        .unwrap()
                        .1;
                    let window = app.waveform_window(&track);
                    let beats = app
                        .detail
                        .as_ref()
                        .unwrap()
                        .beat_grid
                        .visible(window.start, window.end, usize::MAX)
                        .collect::<Vec<_>>();
                    assert!(!lines.is_empty());
                    assert!(lines.len() <= (rect.width() / 32.0).floor() as usize);
                    if zoom <= 8.0 {
                        assert!(lines.iter().all(|(_, downbeat)| *downbeat));
                    } else if zoom >= 32.0 {
                        assert_eq!(lines.len(), beats.len(), "zooming in restores every beat");
                        assert!(lines.iter().any(|(_, downbeat)| !*downbeat));
                    }
                    for pair in lines.windows(2) {
                        assert!(
                            pair[1].0[0].x - pair[0].0[0].x >= 31.9,
                            "regular beats should have room at {size:?}, {zoom}x"
                        );
                    }
                    for (line, downbeat) in &lines {
                        assert!(
                            beats.iter().any(|beat| {
                                let expected = rect.left()
                                    + ((beat.seconds() - window.start)
                                        / (window.end - window.start))
                                        as f32
                                        * rect.width();
                                (line[0].x - expected).abs() < 0.01
                                    && *downbeat == (beat.beat_in_bar == 1)
                            }),
                            "lines must remain on saved beat timestamps and bar phase"
                        );
                        assert_eq!(line[0].x, line[1].x);
                        assert!(
                            rect.contains(line[0]) && rect.contains(line[1]),
                            "grid must stay inside the detail waveform"
                        );
                    }
                }
            }
        }
        let size = vec2(1280.0, 800.0);
        click(&mut app, &ctx, size, "Grid");
        assert!(paint(&mut app, size).is_empty());
        assert_eq!(app.zoom, 256.0);
        assert_eq!(app.position, f64::from(track.duration));
    }

    #[test]
    fn waveform_views_handle_loading_missing_and_zero_duration_tracks() {
        let ctx = Context::default();
        let size = vec2(800.0, 480.0);
        let mut app = cards_app(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        let mut track = app.tracks[0].clone();
        track.duration = 0;
        app.inspect(track.clone());
        app.detail.as_mut().unwrap().waveform.entries.clear();
        click(&mut app, &ctx, size, "Grid");
        assert!(
            app.show_grid,
            "Grid toggle must be disabled when no grid is available"
        );
        click(&mut app, &ctx, size, "Full waveform");
        click(&mut app, &ctx, size, "Detail waveform");
        assert_eq!(app.position, 0.0);
        assert!(app.waveform_window(&track).end.is_finite());
        app.detail = None;
        for busy in [true, false] {
            app.detail_busy = busy;
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.hits.iter().any(|(name, _)| name == "Full waveform"));
            assert!(app.hits.iter().any(|(name, _)| name == "Detail waveform"));
        }
    }

    #[test]
    fn search_sort_and_demo_load_keep_track_identity() {
        let ctx = Context::default();
        let size = vec2(800.0, 480.0);
        let mut app = cards_app(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        app.query = "MIRA RAIN".into();
        app.filter();
        assert_eq!(app.visible.len(), 8);
        assert!(
            app.visible
                .iter()
                .all(|&i| app.tracks[i].artist == "Mira Vale")
        );
        app.order.field = crate::library::Sort::Bpm;
        app.filter();
        app.selected = Some(1);
        app.load_dialog = true;
        frame(&mut app, &ctx, size, vec![]);
        click(&mut app, &ctx, size, "Load 1");
        assert!(
            app.load_confirmation.is_some(),
            "playing deck must ask before loading"
        );
        frame(&mut app, &ctx, size, vec![]);
        click(&mut app, &ctx, size, "Load cancel");
        click(&mut app, &ctx, size, "Load 2");
        assert!(!app.load_dialog);
        assert_eq!(app.status.players[1].track_id, 1);
    }
}
