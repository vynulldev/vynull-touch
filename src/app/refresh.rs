// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::api::refresh::{Cues, Request, Snapshot};
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

const INTERVAL: Duration = Duration::from_secs(5);

pub(super) struct State {
    pub generation: u64,
    pub tracks_revision: Option<u64>,
    cues_revision: Option<u64>,
    cues: Cues,
    pub skip_cues: bool,
    pub inflight: bool,
    pub force: bool,
    pub next: Instant,
    pub pending: Option<(Request, Snapshot)>,
    pub error: String,
    pub checked: Option<Instant>,
    failures: u32,
}

impl Default for State {
    fn default() -> Self {
        Self {
            generation: 0,
            tracks_revision: None,
            cues_revision: None,
            cues: Cues::new(),
            skip_cues: false,
            inflight: false,
            force: false,
            next: Instant::now() + INTERVAL,
            pending: None,
            error: String::new(),
            checked: None,
            failures: 0,
        }
    }
}

impl State {
    pub(super) fn cues(&self, track: u32) -> &[Cue] {
        self.cues.get(&track).map_or(&[], Vec::as_slice)
    }

    // Foreground reads and verified writes are newer than the last batch.
    // Keep its revision unchanged so the next refresh still fetches any edits.
    pub(super) fn remember_cues(&mut self, track: u32, cues: &[Cue]) {
        self.cues.insert(track, cues.to_vec());
    }
}

impl App {
    fn refresh_busy(&self, ctx: &Context) -> bool {
        // Include the release frame: the underlying row/pad still has to
        // process its click against exactly what was drawn under the finger.
        ctx.input(|i| {
            i.pointer.any_down()
                || i.pointer.any_released()
                || i.smooth_scroll_delta() != Vec2::ZERO
        }) || (self.page == Page::Collection
            && self
                .library_scroll
                .is_some_and(|id| track_scroll::moving(ctx, id)))
            || self.keyboard.is_some()
            || self.filter_dialog.is_some()
            || self.sort_dialog.is_some()
            || self.cue_editor.is_some()
            || self.cue_quick.is_some()
            || self.cue_press.is_some()
            || self.wave_drag_start.is_some()
            || self.preview.request.dragging
            || self.load_dialog
            || self.load_confirmation.is_some()
            || self.load_busy
            || self.pending_load.is_some()
            || self.loading
            || self.detail_busy
            || self.editor.is_some()
            || self
                .grid_editor
                .as_ref()
                .is_some_and(|e| e.open || e.pending)
    }

    pub(super) fn refresh_failed(&mut self, force: bool, error: String) {
        self.refresh.failures = self.refresh.failures.saturating_add(1);
        self.refresh.next = Instant::now()
            + Duration::from_secs(
                (5_u64 << self.refresh.failures.saturating_sub(1).min(3)).min(30),
            );
        self.refresh.force |= force;
        self.refresh.error = format!("Library refresh delayed; will retry. {error}");
    }

    pub(super) fn refresh_tick(&mut self, ctx: &Context) {
        if self.refresh_busy(ctx) {
            return;
        }
        if let Some((request, snapshot)) = self.refresh.pending.take() {
            if request.generation == self.refresh.generation && request.token == self.token {
                self.apply_refresh(ctx, &request, snapshot);
                self.refresh.next = Instant::now() + INTERVAL;
                self.refresh.error.clear();
                self.refresh.checked = Some(Instant::now());
                self.refresh.failures = 0;
            } else {
                // Navigation or a foreground read/write superseded this result.
                self.refresh.force |= request.force;
                self.refresh.next = Instant::now();
            }
        }
        let automatic = self.library_session.is_some() && self.refresh.tracks_revision.is_some();
        if self.options.demo || !self.library_ready || (!automatic && !self.refresh.force) {
            return;
        }
        ctx.request_repaint_after(
            self.refresh
                .next
                .saturating_duration_since(Instant::now())
                .max(Duration::from_millis(100)),
        );
        if self.refresh.inflight || Instant::now() < self.refresh.next || !self.fresh() {
            return;
        }
        let request = Request {
            generation: self.refresh.generation,
            token: self.token,
            source: self.source,
            detail: self.detail_track.as_ref().map(|t| t.id),
            tracks_revision: self.refresh.tracks_revision,
            cues_revision: self.refresh.cues_revision,
            skip_cues: self.refresh.skip_cues,
            force: self.refresh.force,
        };
        if self
            .worker
            .as_ref()
            .is_some_and(|worker| worker.refresh.try_send(request).is_ok())
        {
            self.refresh.inflight = true;
            self.refresh.force = false;
        }
    }

    fn apply_refresh(&mut self, ctx: &Context, request: &Request, snapshot: Snapshot) {
        // Anchor the same track to the same pixel, even when rows above it were
        // inserted, removed or reordered by metadata changes.
        let offset = self
            .library_scroll
            .map_or(0.0, |id| track_scroll::offset(ctx, id));
        let index = (offset / 50.0).floor() as usize;
        let anchor = self
            .visible
            .get(index)
            .map(|&i| (self.tracks[i].id, self.tracks[i].file_path.clone()));
        let mut invalid = HashSet::new();
        let collection_changed = snapshot.tracks.is_some();
        if let Some(tracks) = snapshot.tracks {
            let old: HashMap<_, _> = self.all_tracks.iter().map(|t| (t.id, t)).collect();
            let new: HashMap<_, _> = tracks.iter().map(|t| (t.id, t)).collect();
            for track in &self.all_tracks {
                if new
                    .get(&track.id)
                    .is_none_or(|new| new.file_path != track.file_path)
                {
                    self.refresh.cues.remove(&track.id);
                }
                if new.get(&track.id).copied() != Some(track) {
                    invalid.insert(track.id);
                    if let Some(cache) = &mut self.artwork {
                        cache.invalidate(track);
                    }
                }
            }
            for track in &tracks {
                if old.get(&track.id).copied() != Some(track) {
                    invalid.insert(track.id);
                    if let Some(cache) = &mut self.artwork {
                        cache.invalidate(track);
                    }
                }
            }
            // The revision has no per-track analysis IDs. Grid-only changes
            // can leave list metadata identical, so expire waveform previews
            // and currently monitored library assets on every revision change.
            if let Some(cache) = &mut self.artwork {
                cache.invalidate_waveforms();
            }
            invalid.extend(
                self.status
                    .players
                    .iter()
                    .filter(|p| !p.external && p.source.is_empty())
                    .map(|p| p.track_id),
            );
            self.all_tracks = tracks;
            self.matching.dirty = true;
        }
        self.playlists = snapshot.playlists;
        if self.folder != 0
            && !self
                .playlists
                .iter()
                .any(|p| p.id == self.folder && p.is_folder)
        {
            self.folder = 0;
        }
        let mut playlist_removed = false;
        if let Some(id) = self.source {
            if let Some(playlist) = self.playlists.iter().find(|p| p.id == id && !p.is_folder) {
                self.source_name = playlist.name.clone();
            } else {
                self.source = None;
                playlist_removed = true;
                self.source_name = "All tracks".into();
                self.notice = "This playlist was removed. Showing All tracks.".into();
            }
        }
        if let (Some(id), Some(tracks)) = (self.source, &snapshot.playlist_tracks) {
            self.playlist_counts.remember(id, tracks.len());
        }
        let tracks = if self.source.is_none() {
            (collection_changed || playlist_removed).then(|| self.all_tracks.clone())
        } else {
            snapshot.playlist_tracks
        };
        if let Some(tracks) = tracks
            && tracks != self.tracks
        {
            // Reusing an ID with a different file is not the same selection.
            if self.selected.is_some_and(|id| {
                self.tracks.iter().find(|t| t.id == id).is_some_and(|old| {
                    !tracks
                        .iter()
                        .any(|t| t.id == id && t.file_path == old.file_path)
                })
            }) {
                self.selected = None;
            }
            self.tracks = tracks;
            self.reindex();
            if let Some(scroll) = self.library_scroll {
                let offset = anchor
                    .and_then(|(id, path)| {
                        self.visible.iter().position(|&i| {
                            self.tracks[i].id == id && self.tracks[i].file_path == path
                        })
                    })
                    .map_or(offset, |row| row as f32 * 50.0 + offset % 50.0);
                track_scroll::set_offset(ctx, scroll, offset);
            }
        }
        if let Some(track) = self.detail_track.as_ref() {
            if let Some(updated) = self
                .all_tracks
                .iter()
                .find(|t| t.id == track.id && t.file_path == track.file_path)
            {
                if updated.file_missing && !track.file_missing {
                    self.preview.stop();
                }
                self.detail_track = Some(updated.clone());
            } else {
                self.preview.stop();
                self.detail_track = None;
                self.detail = None;
                self.loop_controls = false;
                if self.page == Page::Detail {
                    self.leave_inspect();
                }
                self.notice =
                    "Inspected track was removed or its file changed. Select it again.".into();
            }
        }
        if let Some(cues) = snapshot.cues {
            for &id in self.refresh.cues.keys().chain(cues.keys()) {
                if self.refresh.cues.get(&id) != cues.get(&id) {
                    invalid.insert(id);
                    if let Some(cache) = &mut self.deck_cache {
                        cache.update_cues(id, cues.get(&id).map_or(&[], Vec::as_slice));
                    }
                }
            }
            if let (Some(track), Some(detail)) = (&self.detail_track, &mut self.detail) {
                detail.cues = cues.get(&track.id).cloned().unwrap_or_default();
                detail.cues_ready = true;
            }
            self.refresh.cues = cues;
        }
        if self.detail_track.is_some()
            && let Some(detail) = snapshot.detail
        {
            if detail.cues_ready {
                self.refresh
                    .remember_cues(self.detail_track.as_ref().unwrap().id, &detail.cues);
            }
            if detail.warning.is_empty() || self.detail.is_none() {
                self.detail = Some(detail);
            } else if let Some(current) = &mut self.detail {
                if detail.cues_ready {
                    current.cues = detail.cues;
                    current.cues_ready = true;
                }
                if !detail.waveform.entries.is_empty() {
                    current.waveform = detail.waveform;
                }
                if !detail.beat_grid.is_empty() {
                    current.beat_grid = detail.beat_grid;
                }
                if !detail.bands.entries.is_empty() {
                    current.bands = detail.bands;
                }
                current.warning = detail.warning;
            }
        }
        if request.force {
            if let Some(cache) = &mut self.artwork {
                cache.clear();
            }
            invalid.extend(self.all_tracks.iter().map(|t| t.id));
            self.notice = "Library refreshed".into();
        }
        if let Some(cache) = &mut self.deck_cache {
            for id in invalid {
                cache.invalidate(id);
            }
        }
        self.refresh.tracks_revision = snapshot.tracks_revision;
        self.refresh.cues_revision = snapshot.cues_revision;
        self.refresh.skip_cues = snapshot.skip_cues;
    }
}
