// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    pub(super) fn inspect_header(&mut self, ui: &mut Ui, track: &Track) {
        let compact = ui.available_height() < 480.0;
        let height = if compact { 72.0 } else { 96.0 };
        let (header, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let pending = self.grid_pending();
        let button = |ui: &mut Ui, rect, text: &str| {
            ui.place(
                rect,
                Button::new(RichText::new(text).color(if pending { MUTED } else { TEXT }))
                    .sense(if pending {
                        Sense::hover()
                    } else {
                        Sense::click()
                    })
                    .fill(CARD)
                    .corner_radius(8),
            )
        };
        let back_rect = Rect::from_center_size(
            pos2(header.left() + 24.0, header.center().y),
            vec2(48.0, 48.0),
        );
        let editing_grid = self.grid_editing();
        let back = button(ui, back_rect, "‹").on_hover_text(if editing_grid {
            "Finish grid editing"
        } else {
            "Back"
        });
        self.hit("Back", back.rect);
        if back.clicked() {
            if editing_grid {
                self.finish_grid_editing();
            } else {
                self.leave_inspect();
            }
        }

        let card = Rect::from_min_max(header.min + vec2(56.0, 0.0), header.max);
        let p = ui.painter().with_clip_rect(card.intersect(ui.clip_rect()));
        p.rect_filled(card, 8, PANEL);
        let cover = Rect::from_min_size(card.min + vec2(4.0, 4.0), Vec2::splat(height - 8.0));
        let texture = self.artwork.as_mut().and_then(|cache| cache.texture(track));
        if let Some(texture) = texture {
            Image::new((texture, cover.size()))
                .corner_radius(4)
                .paint_at(ui, cover);
        } else {
            theme::sleeve(&p, cover, track.id);
        }
        self.hit("Inspect artwork", cover);
        if !track.album.trim().is_empty() {
            ui.interact(cover, ui.scope_id().with("inspect-artwork"), Sense::hover())
                .on_hover_text(&track.album);
        }

        let playlists_rect = Rect::from_center_size(
            pos2(card.right() - 62.0, card.center().y),
            vec2(108.0, 48.0),
        );
        let edit_rect = Rect::from_min_size(playlists_rect.min - vec2(88.0, 0.0), vec2(80.0, 48.0));
        let text_left = cover.right() + 12.0;
        let text_width = (edit_rect.left() - 12.0 - text_left).max(1.0);
        let line = |y, height| {
            Rect::from_min_size(pos2(text_left, header.top() + y), vec2(text_width, height))
        };
        let title_rect = line(0.0, if compact { 28.0 } else { 36.0 });
        let artist_rect = line(
            if compact { 28.0 } else { 36.0 },
            if compact { 22.0 } else { 28.0 },
        );
        let title = if track.title.trim().is_empty() {
            "Untitled track"
        } else {
            &track.title
        };
        let artist = if track.artist.trim().is_empty() {
            "Unknown artist"
        } else {
            &track.artist
        };
        theme::text(
            &p,
            title_rect,
            title,
            if compact { 24.0 } else { 28.0 },
            TEXT,
        );
        theme::text(
            &p,
            artist_rect,
            artist,
            if compact { 19.0 } else { 22.0 },
            MUTED,
        );
        self.hit("Inspect title", title_rect);
        self.hit("Inspect artist", artist_rect);
        for (rect, id, text) in [
            (title_rect, "title", title),
            (artist_rect, "artist", artist),
        ] {
            ui.interact(rect, ui.scope_id().with(("inspect", id)), Sense::hover())
                .on_hover_text(text);
        }

        let metadata = line(
            if compact { 50.0 } else { 66.0 },
            if compact { 22.0 } else { 24.0 },
        );
        let bpm_width = if compact { 116.0 } else { 130.0 };
        let bpm = Rect::from_min_size(metadata.min, vec2(bpm_width, metadata.height()));
        let key = Rect::from_min_size(
            metadata.min + vec2(bpm_width + 12.0, 0.0),
            vec2(100.0, metadata.height()),
        );
        let format = Rect::from_min_max(pos2(key.right() + 12.0, metadata.top()), metadata.max);
        let size = if compact { 18.0 } else { 20.0 };
        theme::text(
            &p,
            bpm,
            if track.bpm.is_finite() && track.bpm > 0.0 {
                format!("{:.1} BPM", track.bpm)
            } else {
                "— BPM".into()
            },
            size,
            ORANGE,
        );
        theme::text(
            &p,
            key,
            format!(
                "Key {}",
                if track.key.trim().is_empty() {
                    "—"
                } else {
                    &track.key
                }
            ),
            size,
            TEXT,
        );
        if format.width() > 0.0 {
            theme::text(&p, format, &track.file_type, 14.0, MUTED);
        }
        self.hit("Inspect BPM", bpm);
        self.hit("Inspect key", key);

        if editing_grid {
            let badge = edit_rect.union(playlists_rect);
            p.rect_filled(badge, 8, SELECTED);
            let text = badge.shrink2(vec2(10.0, 0.0));
            theme::text(
                &p,
                text.with_max_y(text.top() + 24.0),
                "EDIT GRID · Free positioning",
                12.0,
                ORANGE,
            );
            theme::text(
                &p,
                text.with_min_y(text.top() + 24.0),
                "Edits save immediately",
                12.0,
                MUTED,
            );
            self.hit("Grid mode", badge);
            return;
        }
        let edit = button(ui, edit_rect, "Edit…").on_hover_text("Edit track");
        self.hit("Edit track", edit.rect);
        if edit.clicked() {
            self.open_track_edits(track.clone());
        }
        let playlists = button(ui, playlists_rect, "Playlists…");
        self.hit("Inspect playlists", playlists.rect);
        if playlists.clicked() {
            self.open_playlist_manager(Some(track.clone()));
        }
    }
}
