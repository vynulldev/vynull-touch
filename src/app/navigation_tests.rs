// SPDX-License-Identifier: GPL-3.0-or-later
use super::tests::{click, frame};
use super::*;

fn escape(app: &mut App, ctx: &Context, size: Vec2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            size,
            vec![eframe::egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }
}

fn touch(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2, phase: TouchPhase) {
    let mut events = vec![
        eframe::egui::Event::PointerMoved(pos),
        eframe::egui::Event::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        },
    ];
    if phase != TouchPhase::Move {
        events.push(eframe::egui::Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    frame(app, ctx, size, events);
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![]);
    let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
    assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
    touch(app, ctx, size, rect.center(), TouchPhase::Start);
    touch(app, ctx, size, rect.center(), TouchPhase::End);
}

fn rows(app: &App) -> Vec<(String, Rect)> {
    app.hits
        .iter()
        .filter(|(name, _)| name.starts_with("Track "))
        .cloned()
        .collect()
}

#[test]
fn inspect_back_and_escape_restore_player_views_and_stop_preview() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        for (compare, focused) in [(false, false), (false, true), (true, true)] {
            for use_escape in [false, true] {
                let ctx = Context::default();
                let mut app = crate::app::tests::cards_app(
                    &ctx,
                    Options {
                        demo: true,
                        ..Default::default()
                    },
                    None,
                );
                let (controller, requests, _results) = Controller::test();
                app.preview = controller;
                // Browsing a different track must not replace the load selection.
                app.selected = Some(3);
                app.deck_pair = [2, 1];
                app.deck_span = 32.0;
                tap(&mut app, &ctx, size, "Players");
                if compare {
                    tap(&mut app, &ctx, size, "Deck compare");
                    tap(&mut app, &ctx, size, "Compare player 1");
                } else if focused {
                    tap(&mut app, &ctx, size, "Deck focus 1");
                }
                let opener = if focused {
                    "Player inspect"
                } else if use_escape {
                    "Deck inspect 1"
                } else {
                    "Deck title 1"
                };
                tap(&mut app, &ctx, size, opener);
                assert_eq!(app.page, Page::Detail);
                assert_eq!(app.navigation_page(), Page::Players);
                assert_eq!(app.detail_track.as_ref().unwrap().id, 1);
                assert!(!requests.borrow().playing);
                tap(&mut app, &ctx, size, "Preview play");
                assert!(requests.borrow().playing);
                if use_escape {
                    escape(&mut app, &ctx, size);
                } else {
                    tap(&mut app, &ctx, size, "Back");
                }
                assert_eq!(app.page, Page::Players);
                assert_eq!(app.deck_focus, focused.then_some(1));
                assert_eq!(app.deck_compare, compare);
                assert_eq!(app.deck_pair, [2, 1]);
                assert_eq!(app.deck_span, 32.0);
                assert_eq!(app.selected, Some(3));
                assert!(requests.borrow().track.is_none());
                assert!(!requests.borrow().playing);
                assert!(app.status.players[0].is_playing);
                assert_eq!(app.status.players[0].track_id, 1);
                frame(&mut app, &ctx, size, vec![]);
                assert!(app.hits.iter().any(|(name, _)| name == opener));
                if focused {
                    tap(&mut app, &ctx, size, "Player overview");
                    let label = if compare {
                        "Compare player 1"
                    } else {
                        "Deck title 1"
                    };
                    frame(&mut app, &ctx, size, vec![]);
                    assert!(app.hits.iter().any(|(name, _)| name == label));
                }
            }
        }
    }
}

#[test]
fn inspect_round_trips_preserve_filtered_playlist_selection_and_touch_scroll() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        for use_escape in [false, true] {
            let ctx = Context::default();
            let mut app = crate::app::tests::cards_app(
                &ctx,
                Options {
                    demo: true,
                    ..Default::default()
                },
                None,
            );
            let (controller, requests, _results) = Controller::test();
            app.preview = controller;
            tap(&mut app, &ctx, size, "Playlists");
            tap(&mut app, &ctx, size, "Warm-up selections");
            app.query = "Mira".into();
            app.filters.min_bpm = Some(122.0);
            app.order = Order {
                field: crate::library::Sort::Bpm,
                descending: true,
            };
            app.filter();
            frame(&mut app, &ctx, size, vec![]);
            let initial_rows = rows(&app);
            let start = initial_rows[0].1.center();
            touch(&mut app, &ctx, size, start, TouchPhase::Start);
            for step in 1..=6 {
                touch(
                    &mut app,
                    &ctx,
                    size,
                    start - vec2(0.0, step as f32 * 20.0),
                    TouchPhase::Move,
                );
            }
            // Let the finger stop before lifting; the scroll position is exact.
            for _ in 0..20 {
                frame(&mut app, &ctx, size, vec![]);
            }
            touch(
                &mut app,
                &ctx,
                size,
                start - vec2(0.0, 120.0),
                TouchPhase::End,
            );
            frame(&mut app, &ctx, size, vec![]);
            let scrolled_rows = rows(&app);
            assert_ne!(scrolled_rows, initial_rows);
            let selected = scrolled_rows[1]
                .0
                .strip_prefix("Track ")
                .unwrap()
                .parse::<u32>()
                .unwrap();
            tap(&mut app, &ctx, size, &format!("Track {selected}"));
            assert_eq!(app.selected, Some(selected));
            frame(&mut app, &ctx, size, vec![]);
            let before = rows(&app);
            let filters = app.filters.clone();
            let order = app.order;
            let visible = app.visible.clone();
            tap(&mut app, &ctx, size, "Inspect");
            assert_eq!(app.navigation_page(), Page::Collection);
            assert_eq!(app.detail_track.as_ref().unwrap().id, selected);
            if use_escape {
                escape(&mut app, &ctx, size);
            } else {
                tap(&mut app, &ctx, size, "Back");
            }
            assert_eq!(app.page, Page::Collection);
            frame(&mut app, &ctx, size, vec![]);
            assert_eq!(rows(&app), before);

            // Inspecting a playing track must leave this browsing context intact too.
            tap(&mut app, &ctx, size, "Players");
            tap(&mut app, &ctx, size, "Deck title 1");
            assert_eq!(app.detail_track.as_ref().unwrap().id, 1);
            tap(&mut app, &ctx, size, "Back");
            assert_eq!(app.page, Page::Players);
            tap(&mut app, &ctx, size, "Collection");
            frame(&mut app, &ctx, size, vec![]);
            assert_eq!(rows(&app), before);
            assert_eq!(app.source, Some(1));
            assert_eq!(app.source_name, "Warm-up selections");
            assert_eq!(app.query, "Mira");
            assert_eq!(app.order, order);
            assert_eq!(app.filters, filters);
            assert_eq!(app.visible, visible);
            assert_eq!(app.selected, Some(selected));
            assert!(requests.borrow().track.is_none());
        }
    }
}

#[test]
fn inspect_escape_closes_cue_keyboard_and_editor_before_returning_to_origin() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = crate::app::tests::cards_app(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    let (controller, _requests, _results) = Controller::test();
    app.preview = controller;
    click(&mut app, &ctx, size, "Players");
    click(&mut app, &ctx, size, "Deck title 1");
    click(&mut app, &ctx, size, "Edit cues");
    click(&mut app, &ctx, size, "Cue label");
    escape(&mut app, &ctx, size);
    assert_eq!(app.page, Page::Detail);
    assert!(!app.cue_editor.as_ref().unwrap().typing);
    escape(&mut app, &ctx, size);
    assert_eq!(app.page, Page::Detail);
    assert!(app.cue_editor.is_none());
    escape(&mut app, &ctx, size);
    assert_eq!(app.page, Page::Players);

    click(&mut app, &ctx, size, "Deck title 1");
    // Explicit tab navigation replaces the old return destination.
    click(&mut app, &ctx, size, "Collection");
    click(&mut app, &ctx, size, "Track 1");
    click(&mut app, &ctx, size, "Inspect");
    // Loading another Inspect track must not create a return-to-Inspect loop.
    app.inspect(app.tracks[1].clone());
    escape(&mut app, &ctx, size);
    assert_eq!(app.page, Page::Collection);

    click(&mut app, &ctx, size, "Players");
    click(&mut app, &ctx, size, "Deck title 1");
    app.connect(&ctx);
    frame(&mut app, &ctx, size, vec![]);
    assert_eq!(
        app.page,
        Page::Collection,
        "a new connection discards the old destination"
    );
}
