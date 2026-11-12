// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use crate::matching::live::Reference;
use eframe::egui::Event as InputEvent;

fn setup() -> (App, Context) {
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
    app.status.players[0].bpm = 120.0;
    app.status.players[0].pitch_pct = 10.0;
    app.status.players[0].key = "8A".into();
    app.status.players[1] = Player {
        device_number: 2,
        is_master: false,
        bpm: 100.0,
        pitch_pct: 0.0,
        key: "3A".into(),
        ..app.status.players[0].clone()
    };
    for (track, (bpm, key)) in
        app.all_tracks
            .iter_mut()
            .zip([(132.0, "8A"), (132.0, "3A"), (100.0, "8B"), (66.0, "")])
    {
        track.bpm = bpm;
        track.key = key.into();
    }
    app.source(None);
    (app, ctx)
}

fn has(app: &App, label: &str) -> bool {
    app.hits.iter().any(|(name, _)| name == label)
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![InputEvent::PointerGone]);
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("Missing {label}: {:?}", app.hits))
        .1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
        "{label}: {rect:?}"
    );
    for phase in [TouchPhase::Start, TouchPhase::End] {
        let pos = rect.center();
        frame(
            app,
            ctx,
            size,
            vec![
                InputEvent::PointerMoved(pos),
                InputEvent::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos,
                    force: None,
                },
                InputEvent::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: phase == TouchPhase::Start,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn collection_playlist_and_search_highlight_key_and_pitched_bpm_independently() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let (mut app, ctx) = setup();
        for playlist in [false, true] {
            if playlist {
                let mut source = app.playlists[0].clone();
                source.track_ids = vec![1, 2, 3, 4];
                app.source(Some(source));
            }
            app.selected = Some(1);
            let order = app.visible.clone();
            for searching in [false, true] {
                app.keyboard = searching.then_some(Field::Search);
                frame(&mut app, &ctx, size, vec![]);
                assert!(has(&app, "BPM match 1") && has(&app, "Key match 1"));
                assert!(has(&app, "BPM match 2") && !has(&app, "Key match 2"));
                if !searching {
                    // Compact search intentionally renders only two full rows.
                    assert!(!has(&app, "BPM match 3") && has(&app, "Key match 3"));
                    assert!(!has(&app, "BPM match 4") && !has(&app, "Key match 4"));
                }
                assert_eq!(app.visible, order);
                assert_eq!(app.selected, Some(1));
                assert!(app.preview.request.track.is_none());
            }
            app.keyboard = None;
            app.matching.half_double = true;
            frame(&mut app, &ctx, size, vec![]);
            assert!(has(&app, "BPM match 4"));
            app.matching.half_double = false;
        }
    }
}

#[test]
fn live_highlights_follow_master_pitch_track_changes_and_clear_for_unavailable_data() {
    let (mut app, ctx) = setup();
    let size = vec2(800.0, 480.0);
    frame(&mut app, &ctx, size, vec![]);
    assert!(has(&app, "Key match 1"));
    app.status.players[0].is_master = false;
    app.status.players[1].is_master = true;
    frame(&mut app, &ctx, size, vec![]);
    assert!(!has(&app, "Key match 1") && has(&app, "Key match 2"));
    assert!(has(&app, "BPM match 3"));
    app.library_match = Reference::Player(1);
    frame(&mut app, &ctx, size, vec![]);
    assert!(has(&app, "Key match 1") && !has(&app, "Key match 2"));
    app.status.players[0].pitch_pct = 0.0;
    frame(&mut app, &ctx, size, vec![]);
    assert!(has(&app, "Key match 1") && !has(&app, "BPM match 1"));
    app.status.players[0].track_id = 999;
    app.status.players[0].key = "3A".into();
    app.status.players[0].bpm = 132.0;
    frame(&mut app, &ctx, size, vec![]);
    assert!(has(&app, "BPM match 2") && has(&app, "Key match 2"));
    app.status.players[0].key.clear();
    frame(&mut app, &ctx, size, vec![]);
    assert!(has(&app, "BPM match 2") && !has(&app, "Key match 2"));
    app.tracks[1].file_missing = true;
    frame(&mut app, &ctx, size, vec![]);
    assert!(!has(&app, "BPM match 2"));
    app.options.demo = false;
    app.last_status = Some(Instant::now() - Duration::from_secs(5));
    frame(&mut app, &ctx, size, vec![]);
    assert!(!app.hits.iter().any(|(name, _)| name.contains(" match ")));
    app.last_status = Some(Instant::now());
    app.linked = false;
    frame(&mut app, &ctx, size, vec![]);
    assert!(!app.hits.iter().any(|(name, _)| name.contains(" match ")));
    app.linked = true;
    app.status.peers.retain(|p| p.device_number != 1);
    frame(&mut app, &ctx, size, vec![]);
    assert!(!app.hits.iter().any(|(name, _)| name.contains(" match ")));
    assert_eq!(
        app.library_match,
        Reference::Player(1),
        "a missing chosen deck cannot switch references"
    );
}

#[test]
fn match_picker_fits_on_touch_screens_and_preserves_browsing_and_audio() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let (mut app, ctx) = setup();
        app.query = "rain".into();
        app.filter();
        app.selected = Some(1);
        let visible = app.visible.clone();
        let revision = app.preview.request.revision;
        frame(&mut app, &ctx, size, vec![]);
        let mut previous_right = 0.0;
        for label in [
            "Search",
            "Clear search",
            "Library match",
            "Library filters",
            "Library sort",
        ] {
            let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
            assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect));
            assert!(rect.left() >= previous_right);
            previous_right = rect.right();
        }
        tap(&mut app, &ctx, size, "Library match");
        tap(&mut app, &ctx, size, "Highlight player 2");
        tap(&mut app, &ctx, size, "Highlight ±3%");
        tap(&mut app, &ctx, size, "Highlight half double");
        assert_eq!(app.library_match, Reference::Player(2));
        assert_eq!(app.matching.tolerance, 3.0);
        assert!(app.matching.half_double);
        let dialog = app
            .hits
            .iter()
            .find(|(name, _)| name == "Highlight dialog")
            .unwrap()
            .1;
        assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(dialog));
        tap(&mut app, &ctx, size, "Highlight Off");
        tap(&mut app, &ctx, size, "Highlight done");
        frame(&mut app, &ctx, size, vec![]);
        assert!(!app.library_match_picker);
        assert!(!app.hits.iter().any(|(name, _)| name.contains(" match ")));
        assert_eq!(app.preview.request.revision, revision);
        assert_eq!(app.visible, visible);
        assert_eq!(app.selected, Some(1));
        tap(&mut app, &ctx, size, "Library match");
        tap(&mut app, &ctx, size, "Highlight Auto · Master");
        frame(
            &mut app,
            &ctx,
            size,
            vec![InputEvent::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
        assert!(!app.library_match_picker);
        assert_eq!(app.page, Page::Collection);
        assert_eq!(app.library_match, Reference::Master);
    }
}
