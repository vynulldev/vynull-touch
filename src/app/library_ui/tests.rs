// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};
use eframe::egui::Event;

#[test]
fn search_previews_update_live_fit_above_keyboard_and_select_in_playlist_scope() {
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
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        app.source(Some(app.playlists[0].clone()));
        app.filters.min_bpm = Some(124.0);
        app.order = Order {
            field: Sort::Duration,
            descending: true,
        };
        app.filter();
        tap(&mut app, &ctx, size, "Search");
        for key in "orbit".chars() {
            tap(&mut app, &ctx, size, &format!("Key {key}"));
        }
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.query, "orbit");
        assert_eq!(app.visible.len(), 8);
        assert_eq!(app.tracks[app.visible[0]].id, 89);
        let rect = |label: &str| app.hits.iter().find(|(name, _)| name == label).unwrap().1;
        let results = rect("Library results");
        assert!(
            results.height() >= 100.0,
            "two touch rows must fit at {size:?}"
        );
        assert!(results.bottom() < rect("Key q").top());
        for id in [89, 77] {
            assert!(results.contains_rect(rect(&format!("Track {id}"))));
            assert!(results.contains_rect(rect(&format!("Preview {id}"))));
            assert!(results.contains_rect(rect(&format!("Artwork {id}"))));
        }
        tap(&mut app, &ctx, size, "Key x");
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.visible.is_empty());
        assert!(app.hits.iter().any(|(name, _)| name == "Library empty"));
        assert!(!app.hits.iter().any(|(name, _)| name.starts_with("Track ")));
        tap(&mut app, &ctx, size, "Keyboard backspace");
        assert_eq!(app.visible.len(), 8);
        tap(&mut app, &ctx, size, "Track 89");
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.keyboard.is_none());
        assert_eq!(app.selected, Some(89));
        assert_eq!(app.page, Page::Collection);
        assert_eq!(app.source, Some(1));
        assert_eq!(app.query, "orbit");
        assert_eq!(app.filters.min_bpm, Some(124.0));
        assert_eq!(
            app.order,
            Order {
                field: Sort::Duration,
                descending: true
            }
        );
        assert!(app.hits.iter().any(|(name, _)| name == "Inspect"));
        assert!(requests.borrow().track.is_none());
        assert!(!app.load_dialog);
    }
}

#[test]
fn search_preview_drag_does_not_select_and_physical_typing_and_done_keep_results() {
    for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        tap(&mut app, &ctx, size, "Search");
        frame(&mut app, &ctx, size, vec![Event::Text("rain".into())]);
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.visible.len(), 8);
        let results = app
            .hits
            .iter()
            .find(|(name, _)| name == "Library results")
            .unwrap()
            .1;
        let start = results.center_bottom() - vec2(0.0, 15.0);
        for (phase, dy) in [
            (TouchPhase::Start, 0.0),
            (TouchPhase::Move, 30.0),
            (TouchPhase::Move, 60.0),
            (TouchPhase::End, 60.0),
        ] {
            let pos = start - vec2(0.0, dy);
            let mut events = vec![
                Event::PointerMoved(pos),
                Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(2),
                    phase,
                    pos,
                    force: None,
                },
            ];
            if phase != TouchPhase::Move {
                events.push(Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: phase == TouchPhase::Start,
                    modifiers: Modifiers::NONE,
                });
            }
            frame(&mut app, &ctx, size, events);
        }
        assert!(track_scroll::offset(&ctx, app.library_scroll.unwrap()) > 0.0);
        assert!(app.keyboard == Some(Field::Search));
        assert!(app.selected.is_none());
        tap(&mut app, &ctx, size, "Keyboard done");
        assert!(app.keyboard.is_none());
        assert_eq!(app.query, "rain");
        assert_eq!(app.visible.len(), 8);
        assert!(app.selected.is_none());
        tap(&mut app, &ctx, size, "Search");
        tap(&mut app, &ctx, size, "Clear search");
        assert!(app.keyboard == Some(Field::Search));
        assert!(app.query.is_empty());
        assert_eq!(app.visible.len(), 96);
    }
}

#[test]
fn table_headers_sort_and_preview_taps_select_without_starting_audio() {
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
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        tap(&mut app, &ctx, size, "Preview 1");
        assert_eq!(app.selected, Some(1));
        assert_eq!(app.page, Page::Collection);
        tap(&mut app, &ctx, size, "Artwork 2");
        assert_eq!(app.selected, Some(2));
        assert!(requests.borrow().track.is_none());
        for (label, field) in [
            ("Title", Sort::Title),
            ("Artist", Sort::Artist),
            ("BPM", Sort::Bpm),
            ("Key", Sort::Key),
            ("Duration", Sort::Duration),
        ] {
            tap(&mut app, &ctx, size, &format!("Column {label}"));
            assert_eq!(
                app.order,
                Order {
                    field,
                    descending: false
                }
            );
            tap(&mut app, &ctx, size, &format!("Column {label}"));
            assert_eq!(
                app.order,
                Order {
                    field,
                    descending: true
                }
            );
            assert_eq!(app.selected, Some(2));
        }
        assert_eq!(app.tracks[app.visible[0]].id, 96);
        tap(&mut app, &ctx, size, "Column Duration");
        assert_eq!(app.order, Order::default());
        assert_eq!(app.visible, (0..96).collect::<Vec<_>>());
        assert_eq!(app.selected, Some(2));
        tap(&mut app, &ctx, size, "Column Duration");
        assert_eq!(
            app.order,
            Order {
                field: Sort::Duration,
                descending: false
            }
        );
        let mut playlist = app.playlists[0].clone();
        playlist.track_ids.swap(0, 2);
        let source_order = playlist.track_ids.clone();
        app.source(Some(playlist));
        app.query = "Mira".into();
        app.filter();
        tap(&mut app, &ctx, size, "Column BPM");
        tap(&mut app, &ctx, size, "Column BPM");
        tap(&mut app, &ctx, size, "Column BPM");
        assert_eq!(app.order, Order::default());
        assert_eq!(app.query, "Mira");
        assert_eq!(
            app.visible
                .iter()
                .map(|&i| app.tracks[i].id)
                .collect::<Vec<_>>(),
            source_order
        );
        tap(&mut app, &ctx, size, "Clear search");
        assert_eq!(app.visible.len(), 24);
        tap(&mut app, &ctx, size, "All tracks");
        assert!(app.source.is_none());
        assert_eq!(app.visible.len(), 96);
    }
}

#[test]
fn touch_sort_and_filters_fit_cancel_apply_and_keep_track_identity() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
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
        app.selected = Some(1);
        tap(&mut app, &ctx, size, "Library sort");
        tap(&mut app, &ctx, size, "Sort Artist");
        tap(&mut app, &ctx, size, "Sort Descending");
        tap(&mut app, &ctx, size, "Sort cancel");
        assert_eq!(app.order, Order::default());
        tap(&mut app, &ctx, size, "Library sort");
        tap(&mut app, &ctx, size, "Sort BPM");
        tap(&mut app, &ctx, size, "Sort Descending");
        tap(&mut app, &ctx, size, "Sort apply");
        assert_eq!(app.tracks[app.visible[0]].bpm, 131.0);
        assert_eq!(app.selected, Some(1));
        assert_eq!(app.tracks[0].id, 1);
        tap(&mut app, &ctx, size, "Library filters");
        for key in ["1", "2", "8", ".", "5"] {
            tap(&mut app, &ctx, size, &format!("BPM key {key}"));
        }
        tap(&mut app, &ctx, size, "Maximum BPM");
        for key in ["1", "2", "0"] {
            tap(&mut app, &ctx, size, &format!("BPM key {key}"));
        }
        tap(&mut app, &ctx, size, "Filters apply");
        assert!(app.filter_dialog.is_some(), "invalid range must not apply");
        tap(&mut app, &ctx, size, "BPM clear");
        tap(&mut app, &ctx, size, "Filter tab Genre");
        tap(&mut app, &ctx, size, "Filter Genre Deep House");
        tap(&mut app, &ctx, size, "Filter tab Rating");
        tap(&mut app, &ctx, size, "Filter Rating 3 stars or more");
        tap(&mut app, &ctx, size, "Filters apply");
        assert_eq!(app.filters.min_bpm, Some(128.5));
        assert_eq!(app.filters.genres, ["deep house"]);
        assert!(
            app.selected.is_none(),
            "hidden selection must not remain loadable"
        );
        assert!(!app.visible.is_empty());
        assert!(app.visible.iter().all(|&i| app.tracks[i].bpm >= 128.5
            && app.tracks[i].genre == "Deep House"
            && app.tracks[i].rating >= 3));
        let id = app.tracks[app.visible[0]].id;
        tap(&mut app, &ctx, size, &format!("Track {id}"));
        assert_eq!(app.selected, Some(id));
        app.load_dialog = true;
        tap(&mut app, &ctx, size, "Load 2");
        assert_eq!(app.status.players[1].track_id, id);
        tap(&mut app, &ctx, size, "Library filters");
        tap(&mut app, &ctx, size, "Filters reset");
        tap(&mut app, &ctx, size, "Filters cancel");
        assert_eq!(app.filters.count(), 3);
        tap(&mut app, &ctx, size, "Clear filters");
        assert_eq!(app.filters.count(), 0);
        assert_eq!(app.visible.len(), 96);
        assert_eq!(app.selected, Some(id));
        app.filters.keys = vec!["1b".into()];
        app.query = "MIRA RAIN".into();
        app.filter();
        assert_eq!(app.visible.len(), 8);
        let playlist = app.playlists[1].clone();
        app.source(Some(playlist));
        assert_eq!(app.filters.keys, ["1b"]);
        assert!(app.visible.is_empty());
        assert!(app.selected.is_none());
        assert!(requests.borrow().track.is_none());
    }
}

#[test]
fn filter_choices_combine_keys_colors_and_literal_any_tags_and_escape_discards_drafts() {
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
    app.all_tracks[0].tags = vec!["Any".into()];
    app.tracks[0].tags = vec!["Any".into()];
    app.reindex();
    tap(&mut app, &ctx, size, "Library filters");
    tap(&mut app, &ctx, size, "Filter tab Key");
    tap(&mut app, &ctx, size, "Filter Key 1B");
    tap(&mut app, &ctx, size, "Filter Key 2A");
    tap(&mut app, &ctx, size, "Filter tab Color");
    tap(&mut app, &ctx, size, "Filter Color No color");
    tap(&mut app, &ctx, size, "Filter Color Pink");
    tap(&mut app, &ctx, size, "Filter tab Tags");
    tap(&mut app, &ctx, size, "Filter Tags Any");
    tap(&mut app, &ctx, size, "Filters apply");
    assert_eq!(app.visible, [0]);
    assert_eq!(app.filters.tags, ["any"]);
    let applied = app.filters.clone();
    tap(&mut app, &ctx, size, "Library filters");
    tap(&mut app, &ctx, size, "Filters reset");
    frame(
        &mut app,
        &ctx,
        size,
        vec![eframe::egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    assert!(app.filter_dialog.is_none());
    assert_eq!(app.filters, applied);
    app.connect(&ctx);
    assert_eq!(app.filters, Filters::default());
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![eframe::egui::Event::PointerGone]);
    frame(app, ctx, size, vec![]);
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("Missing {label}: {:?}", app.hits))
        .1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
        "{label} outside screen: {rect:?}"
    );
    for (name, dialog) in app
        .hits
        .iter()
        .filter(|(name, _)| name == "Sort dialog" || name == "Filters dialog")
    {
        assert!(
            Rect::from_min_size(Pos2::ZERO, size).contains_rect(*dialog),
            "{name} outside screen: {dialog:?}"
        );
    }
    for phase in [TouchPhase::Start, TouchPhase::End] {
        frame(
            app,
            ctx,
            size,
            vec![
                eframe::egui::Event::PointerMoved(rect.center()),
                eframe::egui::Event::PointerButton {
                    pos: rect.center(),
                    button: PointerButton::Primary,
                    pressed: phase == TouchPhase::Start,
                    modifiers: Modifiers::NONE,
                },
                eframe::egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos: rect.center(),
                    force: None,
                },
            ],
        );
    }
}

#[test]
fn popup_backspace_is_to_the_right_of_p_for_search_server_and_cues() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            None,
        );
        for field in [Field::Search, Field::Server] {
            app.page = if field == Field::Search {
                Page::Collection
            } else {
                Page::Settings
            };
            app.keyboard = Some(field);
            app.symbols = false;
            app.query = "éa".into();
            app.server_draft = "http://pi/éa".into();
            for symbols in [false, true] {
                app.symbols = symbols;
                frame(&mut app, &ctx, size, vec![]);
                frame(&mut app, &ctx, size, vec![]);
                let rect = |label: &str| app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                let p = rect(if symbols { "Key 0" } else { "Key p" });
                let backspace = rect("Keyboard backspace");
                assert!(p.right() < backspace.left());
                assert_eq!(p.top(), backspace.top());
                tap(&mut app, &ctx, size, "Keyboard backspace");
            }
            assert_eq!(
                if field == Field::Search {
                    &app.query
                } else {
                    &app.server_draft
                },
                if field == Field::Search {
                    ""
                } else {
                    "http://pi/"
                }
            );
            app.keyboard = None;
        }
        app.inspect(app.all_tracks[0].clone());
        click(&mut app, &ctx, size, "Edit cues");
        click(&mut app, &ctx, size, "Cue label");
        app.symbols = false;
        app.cue_editor.as_mut().unwrap().draft.label = "café".into();
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let rect = |label: &str| app.hits.iter().find(|(name, _)| name == label).unwrap().1;
        assert!(rect("Cue key p").right() < rect("Cue backspace").left());
        assert_eq!(rect("Cue key p").top(), rect("Cue backspace").top());
        tap(&mut app, &ctx, size, "Cue backspace");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.label, "caf");
    }
}
