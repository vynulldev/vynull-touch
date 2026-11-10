// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::editing::{self, Catalog, Category, Reply, Request, Tag, TrackSnapshot};
use std::collections::BTreeSet;

mod catalog;
mod playlists;
mod tag_picker;
mod tag_widgets;
#[cfg(test)]
mod tests;

#[derive(Clone)]
pub(super) enum View {
    Quick,
    TrackTags,
    Tags,
    Categories,
    TagForm {
        before: Option<Tag>,
        name: String,
        category: u32,
    },
    CategoryForm {
        before: Option<Category>,
        name: String,
    },
    Playlists,
    PlaylistForm {
        before: Option<Playlist>,
        name: String,
        parent: u32,
        folder: bool,
    },
    Members {
        before: Playlist,
        ids: Vec<u32>,
        selected: Option<usize>,
    },
}

pub(super) struct Draft {
    before: TrackSnapshot,
    rating: u8,
    color: u8,
    tags: BTreeSet<u32>,
}

impl Draft {
    fn new(before: TrackSnapshot) -> Self {
        Self {
            rating: before.track.rating,
            color: before.track.color_id,
            tags: before.tags.clone(),
            before,
        }
    }
    fn changed(&self) -> bool {
        self.rating != self.before.track.rating
            || self.color != self.before.track.color_id
            || self.tags != self.before.tags
    }
    fn request(&self) -> Request {
        Request::SaveTrack {
            before: Box::new(self.before.clone()),
            rating: self.rating,
            color: self.color,
            tags: self.tags.clone(),
        }
    }
}

pub(super) struct Manager {
    view: View,
    track: Option<Draft>,
    origin: Option<Track>,
    adding: Option<Track>,
    catalog: Catalog,
    playlists: Vec<Playlist>,
    pub pending: bool,
    pub saving: bool,
    ready: bool,
    token: u64,
    error: String,
    message: String,
    reload: bool,
    pub typing: bool,
    query: String,
    tag_picker: tag_picker::State,
    catalog_browser: catalog::State,
    confirm: Option<(String, Request)>,
    after: Option<View>,
}

impl Manager {
    fn new(view: View) -> Self {
        Self {
            view,
            track: None,
            origin: None,
            adding: None,
            catalog: Catalog::default(),
            playlists: vec![],
            pending: false,
            saving: false,
            ready: false,
            token: 0,
            error: String::new(),
            message: String::new(),
            reload: false,
            typing: false,
            query: String::new(),
            tag_picker: tag_picker::State::default(),
            catalog_browser: catalog::State::default(),
            confirm: None,
            after: None,
        }
    }
}

impl App {
    pub(super) fn open_track_edits(&mut self, track: Track) {
        let mut manager = Manager::new(View::Quick);
        manager.origin = Some(track.clone());
        self.preview.pause();
        self.edit_submit(&mut manager, Request::Track(track));
        self.editor = Some(manager);
    }

    pub(super) fn open_tag_manager(&mut self) {
        let mut manager = Manager::new(View::Tags);
        self.edit_submit(&mut manager, Request::Catalog);
        self.editor = Some(manager);
    }

    pub(super) fn open_playlist_manager(&mut self, adding: Option<Track>) {
        let mut manager = Manager::new(View::Playlists);
        manager.adding = adding;
        self.edit_submit(&mut manager, Request::Playlists);
        self.editor = Some(manager);
    }

    fn edit_submit(&mut self, manager: &mut Manager, request: Request) {
        if manager.pending {
            return;
        }
        manager.error.clear();
        manager.message.clear();
        manager.confirm = None;
        self.edit_serial = self.edit_serial.wrapping_add(1);
        manager.token = self.edit_serial;
        manager.saving = request.writes();
        if request.writes() {
            self.refresh.generation = self.refresh.generation.wrapping_add(1);
            self.refresh.pending = None;
            self.refresh.force = true;
            self.refresh.next = Instant::now();
        }
        if self.options.demo {
            let result = self
                .demo_edits
                .run(&request, &self.all_tracks, &self.playlists);
            self.edit_result(manager, result, request.writes());
        } else {
            manager.pending = self.send(Command::Edit(manager.token, Box::new(request)));
            if !manager.pending {
                manager.error = "Could not queue the request. Try again.".into();
            }
        }
    }

    pub(super) fn edit_response(&mut self, token: u64, result: Result<Reply, String>) {
        if let Some(mut manager) = self.editor.take() {
            if manager.token == token {
                let saved = manager.saving;
                self.edit_result(&mut manager, result, saved);
            }
            self.editor = Some(manager);
        }
    }

    fn edit_result(&mut self, manager: &mut Manager, result: Result<Reply, String>, saved: bool) {
        manager.pending = false;
        match result {
            Err(error) => {
                manager.error = error;
                manager.reload = true;
                manager.after = None;
            }
            Ok(reply) => {
                manager.ready = true;
                manager.reload = false;
                manager.error.clear();
                manager.query.clear();
                if saved {
                    manager.message = "Saved and verified".into();
                }
                match reply {
                    Reply::Track(snapshot) => {
                        self.update_edited_track(&snapshot.track);
                        self.reindex();
                        manager.catalog = snapshot.catalog.clone();
                        manager.origin = Some(snapshot.track.clone());
                        manager.track = Some(Draft::new(*snapshot));
                    }
                    Reply::Catalog(catalog, tracks) => {
                        manager.catalog = catalog;
                        if let Some(tracks) = tracks {
                            let by_id: std::collections::HashMap<_, _> =
                                tracks.iter().map(|t| (t.id, t)).collect();
                            for old in &mut self.tracks {
                                if let Some(track) = by_id.get(&old.id)
                                    && track.file_path == old.file_path
                                {
                                    *old = (*track).clone();
                                }
                            }
                            if let Some(old) = &mut self.detail_track
                                && let Some(track) = by_id.get(&old.id)
                                && track.file_path == old.file_path
                            {
                                *old = (*track).clone();
                            }
                            self.all_tracks = tracks;
                            self.reindex();
                        }
                    }
                    Reply::Playlists(playlists) => {
                        self.apply_edited_playlists(&playlists);
                        manager.playlists = playlists;
                        if manager.adding.is_some() && manager.after.is_some() {
                            manager.message = "Added to playlist".into();
                        }
                    }
                }
                if let Some(view) = manager.after.take() {
                    manager.view = view;
                }
            }
        }
    }

    fn update_edited_track(&mut self, track: &Track) {
        for rows in [&mut self.all_tracks, &mut self.tracks] {
            if let Some(old) = rows
                .iter_mut()
                .find(|t| t.id == track.id && t.file_path == track.file_path)
            {
                *old = track.clone();
            }
        }
        if let Some(old) = &mut self.detail_track
            && old.id == track.id
            && old.file_path == track.file_path
        {
            *old = track.clone();
        }
    }

    pub(super) fn edit_dialog(&mut self, ctx: &Context) {
        let Some(mut manager) = self.editor.take() else {
            return;
        };
        let mut close = false;
        let mut request = None;
        let modal_id = Id::unique("track-management");
        Modal::new(modal_id)
            .area(Modal::default_area(modal_id).anchor(Align2::CENTER_TOP, vec2(0.0, 16.0)))
            .show(ctx, |ui| {
                let width = if matches!(
                    manager.view,
                    View::TrackTags | View::Tags | View::Categories
                ) && !manager.typing
                {
                    1000.0
                } else {
                    720.0
                };
                ui.set_width((ctx.content_rect().width() - 48.0).min(width));
                ui.set_height(ctx.content_rect().height() - 48.0);
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                if manager.typing {
                    ui.heading(match manager.view {
                        View::TagForm { .. } => "Tag name",
                        View::CategoryForm { .. } => "Category name",
                        View::PlaylistForm { folder: true, .. } => "Folder name",
                        View::PlaylistForm { .. } => "Playlist name",
                        _ => "Search",
                    });
                    let value = match &mut manager.view {
                        View::TagForm { name, .. }
                        | View::CategoryForm { name, .. }
                        | View::PlaylistForm { name, .. } => name,
                        _ => &mut manager.query,
                    };
                    ui.add_sized(
                        vec2(ui.available_width(), 48.0),
                        TextEdit::singleline(value).char_limit(128),
                    );
                    if self.edit_keyboard(ui, value) {
                        manager.typing = false;
                    }
                    return;
                }
                ui.horizontal(|ui| {
                    ui.heading(match &manager.view {
                        View::Quick | View::TrackTags => "Edit track",
                        View::Tags | View::TagForm { .. } => "Manage tags",
                        View::Categories | View::CategoryForm { .. } => "Tag categories",
                        View::Members { .. } => "Playlist tracks",
                        _ if manager.adding.is_some() => "Add to playlist",
                        _ => "Manage playlists",
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_enabled_ui(!manager.pending, |ui| {
                            if self.edit_button(ui, "Close", 80.0, false) {
                                close = true;
                            }
                        });
                    });
                });
                if manager.pending {
                    ui.label("Working…");
                }
                if !manager.error.is_empty() {
                    ScrollArea::vertical()
                        .id_salt("edit-error")
                        .max_height(72.0)
                        .show(ui, |ui| {
                            ui.colored_label(ORANGE, &manager.error);
                        });
                }
                if !manager.message.is_empty() {
                    ui.colored_label(GREEN, &manager.message);
                }
                if manager.reload && self.edit_button(ui, "Reload", 110.0, false) {
                    manager.typing = false;
                    manager.confirm = None;
                    request = Some(match &manager.view {
                        View::Quick | View::TrackTags => {
                            Request::Track(manager.origin.clone().unwrap())
                        }
                        View::Tags
                        | View::Categories
                        | View::TagForm { .. }
                        | View::CategoryForm { .. } => {
                            manager.view = View::Tags;
                            Request::Catalog
                        }
                        _ => {
                            manager.view = View::Playlists;
                            Request::Playlists
                        }
                    });
                }
                let writable =
                    self.options.demo || (self.library_session.is_some() && self.fresh());
                if !writable {
                    ui.colored_label(MUTED, "Reconnect before saving changes.");
                }
                let height = (ctx.content_rect().bottom() - 32.0 - ui.cursor().top()).max(80.0);
                if matches!(
                    manager.view,
                    View::TrackTags
                        | View::Tags
                        | View::Categories
                        | View::TagForm { .. }
                        | View::CategoryForm { .. }
                ) && manager.confirm.is_none()
                {
                    ui.allocate_ui_with_layout(
                        vec2(ui.available_width(), height),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.set_min_height(height);
                            ui.add_enabled_ui(
                                !manager.pending && manager.ready && !manager.reload && writable,
                                |ui| {
                                    if matches!(manager.view, View::TrackTags) {
                                        self.track_tag_picker(ui, &mut manager, &mut request);
                                    } else {
                                        self.catalog_controls(ui, &mut manager, &mut request);
                                    }
                                },
                            );
                        },
                    );
                    return;
                }
                ScrollArea::vertical()
                    .id_salt("edit-content")
                    .max_height(height)
                    .scroll_source(scroll_area::ScrollSource::ALL)
                    .show(ui, |ui| {
                        ui.add_enabled_ui(
                            !manager.pending && manager.ready && !manager.reload && writable,
                            |ui| {
                                if let Some((message, action)) = manager.confirm.clone() {
                                    ui.label(message);
                                    ui.horizontal(|ui| {
                                        if self.edit_button(ui, "Cancel deletion", 170.0, false) {
                                            manager.confirm = None;
                                        }
                                        if self.edit_button(ui, "Delete permanently", 200.0, false)
                                        {
                                            request = Some(action);
                                        }
                                    });
                                } else {
                                    match manager.view {
                                        View::Quick | View::TrackTags => {
                                            self.track_edit_controls(ui, &mut manager, &mut request)
                                        }
                                        View::Tags
                                        | View::Categories
                                        | View::TagForm { .. }
                                        | View::CategoryForm { .. } => {
                                            self.catalog_controls(ui, &mut manager, &mut request)
                                        }
                                        _ => self.playlist_edit_controls(
                                            ui,
                                            &mut manager,
                                            &mut request,
                                        ),
                                    }
                                }
                            },
                        );
                    });
            });
        if ctx.input(|i| i.key_pressed(Key::Escape)) && !manager.pending {
            if manager.typing {
                manager.typing = false;
            } else if manager.confirm.is_some() {
                manager.confirm = None;
            } else {
                close = true;
            }
        }
        if let Some(request) = request {
            self.edit_submit(&mut manager, request);
        }
        if !close {
            self.editor = Some(manager);
        }
    }

    fn track_edit_controls(
        &mut self,
        ui: &mut Ui,
        manager: &mut Manager,
        request: &mut Option<Request>,
    ) {
        let Some(draft) = &mut manager.track else {
            return;
        };
        ui.add(Label::new(RichText::new(&draft.before.track.title).size(18.0)).truncate());
        ui.horizontal(|ui| {
            if self.edit_button(
                ui,
                "Rating & color",
                160.0,
                matches!(manager.view, View::Quick),
            ) {
                manager.view = View::Quick;
            }
            if self.edit_button(
                ui,
                "Track tags",
                130.0,
                matches!(manager.view, View::TrackTags),
            ) {
                manager.view = View::TrackTags;
            }
            if self.edit_button(ui, "Cancel edits", 130.0, false) {
                *draft = Draft::new(draft.before.clone());
            }
            ui.add_enabled_ui(draft.changed(), |ui| {
                if self.edit_button(ui, "Save track", 140.0, false) {
                    *request = Some(draft.request());
                }
            });
        });
        if matches!(manager.view, View::Quick) {
            ui.label("Rating");
            ui.horizontal(|ui| {
                for rating in 0..=5 {
                    let label = if rating == 0 {
                        "Unrated".into()
                    } else {
                        format!("{rating} {}", if rating == 1 { "star" } else { "stars" })
                    };
                    if self.edit_button(ui, &label, 94.0, draft.rating == rating) {
                        draft.rating = rating;
                    }
                }
            });
            ui.label("Color");
            ui.horizontal_wrapped(|ui| {
                for (id, name) in crate::library::COLORS.iter().enumerate() {
                    if self.edit_button(ui, name, 110.0, draft.color == id as u8) {
                        draft.color = id as u8;
                    }
                }
            });
        }
    }

    fn edit_button(&mut self, ui: &mut Ui, label: &str, width: f32, selected: bool) -> bool {
        let response = theme::button(ui, label, vec2(width, 48.0), selected);
        self.hit(&format!("Edit {label}"), response.rect);
        response.clicked()
    }

    fn edit_keyboard(&mut self, ui: &mut Ui, text: &mut String) -> bool {
        let width = (ui.available_width() - 128.0 - 50.0) / 10.0;
        ui.spacing_mut().item_spacing = vec2(5.0, 4.0);
        let rows = if self.symbols {
            ["1234567890", "-/:.=_@?&", "+#%~[]()"]
        } else {
            ["qwertyuiop", "asdfghjkl", "zxcvbnm"]
        };
        for (i, row) in rows.iter().enumerate() {
            ui.horizontal(|ui| {
                for key in row.chars() {
                    if self.edit_button(ui, &key.to_string(), width, false)
                        && text.chars().count() < 128
                    {
                        text.push(key);
                    }
                }
                if i == 0 && self.edit_button(ui, "Backspace", 128.0, false) {
                    text.pop();
                }
            });
        }
        let mut done = false;
        ui.horizontal(|ui| {
            if self.edit_button(ui, if self.symbols { "ABC" } else { "123" }, 70.0, false) {
                self.symbols = !self.symbols;
            }
            if self.edit_button(ui, "Clear", 80.0, false) {
                text.clear();
            }
            let space = ui.available_width() - 105.0;
            if self.edit_button(ui, "Space", space, false) && text.chars().count() < 128 {
                text.push(' ');
            }
            done = self.edit_button(ui, "Done", 100.0, false);
        });
        done
    }
}
