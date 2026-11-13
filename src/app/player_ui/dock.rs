// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

impl App {
    /// Keep the selected deck pair visible while browsing, as on the XDJ-AZ.
    /// The keyboard and Inspect retain their full working area.
    pub(in crate::app) fn browse_deck_dock(&mut self, root: &mut Ui) {
        if self.keyboard.is_some()
            || !matches!(
                self.page,
                Page::Collection | Page::Playlists | Page::Prepare
            )
        {
            return;
        }
        Panel::bottom("browse-decks")
            .exact_size(64.0)
            .resizable(false)
            .frame(Frame::new().fill(BG).inner_margin(Margin::symmetric(6, 2)))
            .show(root, |ui| {
                let width = (ui.available_width() - 4.0) / 2.0;
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.horizontal(|ui| {
                    for number in self.deck_pair {
                        let deck = self.deck_snapshot(number);
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
                        self.hit(&format!("Browse deck {number}"), rect);
                        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        p.rect_filled(rect, 0, BG);
                        p.rect_stroke(rect, 0, Stroke::new(1.0, LINE), StrokeKind::Inside);
                        let inner = rect.shrink(3.0);
                        let title_row = inner.with_max_y(inner.top() + 18.0);
                        let badge = title_row.with_max_x(title_row.left() + 22.0);
                        p.rect_filled(badge, 0, if deck.live { BLUE } else { CARD });
                        p.text(
                            badge.center(),
                            Align2::CENTER_CENTER,
                            number.to_string(),
                            FontId::proportional(14.0),
                            TEXT,
                        );
                        let player = deck.player.as_ref();
                        let title = player.map_or("Not connected", |player| {
                            if player.track_id == 0 {
                                "No track loaded"
                            } else if player.track_title.is_empty() {
                                "Waiting for track details"
                            } else {
                                &player.track_title
                            }
                        });
                        theme::text(
                            &p,
                            Rect::from_min_max(
                                title_row.min + vec2(28.0, 0.0),
                                title_row.max - vec2(54.0, 0.0),
                            ),
                            title,
                            13.0,
                            TEXT,
                        );
                        theme::text(
                            &p,
                            title_row.with_min_x(title_row.right() - 48.0),
                            player.map_or("", |player| player.key.as_str()),
                            12.0,
                            MUTED,
                        );
                        let remaining = deck
                            .position
                            .filter(|_| deck.assets.duration > 0.0)
                            .map_or("—:—".into(), |position| {
                                format!("~−{}", time((deck.assets.duration - position).max(0.0)))
                            });
                        let lower = inner.with_min_y(title_row.bottom() + 2.0);
                        let state = focus::player_state(
                            self.fresh(),
                            self.linked,
                            deck.peer.as_ref(),
                            player,
                        );
                        theme::text(
                            &p,
                            lower
                                .with_max_x(lower.left() + 70.0)
                                .with_max_y(lower.top() + 12.0),
                            state,
                            9.0,
                            if deck.live { MUTED } else { ORANGE },
                        );
                        theme::text(
                            &p,
                            lower
                                .with_max_x(lower.left() + 70.0)
                                .with_min_y(lower.top() + 12.0),
                            remaining,
                            17.0,
                            TEXT,
                        );
                        let bpm = lower.with_min_x(lower.right() - 65.0);
                        let master = deck.live && player.is_some_and(|player| player.is_master);
                        theme::text(
                            &p,
                            bpm.with_max_y(bpm.top() + 12.0),
                            if master { "BPM · MASTER" } else { "BPM" },
                            9.0,
                            if master { ORANGE } else { MUTED },
                        );
                        theme::text(
                            &p,
                            bpm.with_min_y(bpm.top() + 12.0),
                            player
                                .filter(|p| p.bpm.is_finite() && p.bpm > 0.0)
                                .map_or("—".into(), |p| format!("{:.1}", p.bpm)),
                            20.0,
                            if master { ORANGE } else { TEXT },
                        );
                        let wave = Rect::from_min_max(
                            lower.min + vec2(74.0, 0.0),
                            lower.max - vec2(70.0, 0.0),
                        );
                        if player.is_some_and(|player| player.track_id > 0) {
                            paint_overview(
                                &p,
                                wave,
                                &deck.assets,
                                deck.position,
                                self.waveform_style,
                                self.waveform_overview,
                                true,
                            );
                        } else {
                            p.hline(wave.x_range(), wave.center().y, Stroke::new(1.0, LINE));
                        }
                        if response.on_hover_text("Open this player").clicked() {
                            self.page = Page::Players;
                            self.deck_focus = Some(number);
                        }
                    }
                });
            });
        // Tiny overview cursors need fewer repaints than the scrolling player view.
        if self.fresh()
            && self.linked
            && self.status.players.iter().any(Player::is_forward_playback)
        {
            root.ctx().request_repaint_after(Duration::from_millis(66));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{click, frame};

    #[test]
    fn browse_dock_keeps_controls_accessible_and_gives_the_keyboard_its_space() {
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
            app.deck_pair = [2, 1];
            for page in [Page::Collection, Page::Playlists, Page::Prepare] {
                app.page = page;
                frame(&mut app, &ctx, size, vec![]);
                frame(&mut app, &ctx, size, vec![]);
                let decks: Vec<_> = app
                    .hits
                    .iter()
                    .filter(|(n, _)| n.starts_with("Browse deck "))
                    .collect();
                assert_eq!(decks.len(), 2);
                assert_eq!(decks[0].0, "Browse deck 2");
                assert!(decks[0].1.right() < decks[1].1.left());
                let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
                for (_, rect) in &decks {
                    assert!(bounds.contains_rect(*rect));
                    assert!(rect.height() >= 48.0);
                }
                if page == Page::Collection {
                    let results = app
                        .hits
                        .iter()
                        .find(|(n, _)| n == "Library results")
                        .unwrap()
                        .1;
                    assert!(
                        results.height() >= 200.0,
                        "retain four touch rows: {results:?}"
                    );
                    assert!(results.bottom() < decks[0].1.top());
                }
            }
            app.page = Page::Collection;
            click(&mut app, &ctx, size, "Search");
            frame(&mut app, &ctx, size, vec![]);
            assert!(!app.hits.iter().any(|(n, _)| n.starts_with("Browse deck ")));
            click(&mut app, &ctx, size, "Keyboard done");
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.hits.iter().any(|(n, _)| n == "Browse deck 2"));
        }
    }

    #[test]
    fn opening_a_docked_player_stops_preview_without_loading_or_losing_browse_state() {
        let size = vec2(800.0, 480.0);
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        app.source(Some(app.playlists[0].clone()));
        let source = app.source;
        click(&mut app, &ctx, size, "Track 1");
        click(&mut app, &ctx, size, "Library preview");
        assert!(requests.borrow().playing);
        click(&mut app, &ctx, size, "Browse deck 1");
        assert_eq!(app.page, Page::Players);
        assert_eq!(app.deck_focus, Some(1));
        assert!(!requests.borrow().playing);
        assert!(app.library_preview.is_none());
        assert!(app.pending_load.is_none() && !app.load_dialog);
        assert_eq!(app.status.players[0].track_id, 1);
        click(&mut app, &ctx, size, "Collection");
        assert_eq!(app.source, source);
        assert_eq!(app.selected, Some(1));
    }
}
