// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

#[test]
fn cue_play_requests_smooth_repaints_in_cards_waveforms_and_focus() {
    for (compare, focus) in [(false, None), (true, None), (false, Some(1))] {
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
        app.page = Page::Players;
        app.deck_compare = compare;
        app.deck_focus = focus;
        for p in &mut app.status.players {
            p.is_playing = false;
            p.play_state = 6;
            p.play_state_name = "CUED".into();
        }
        let render = |app: &mut App| {
            let mut delay = Duration::ZERO;
            for _ in 0..20 {
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 480.0))),
                        ..Default::default()
                    },
                    |ui| app.render(ui),
                );
                delay = output.viewport_output[&ViewportId::ROOT].repaint_delay;
                output.textures_delta.clear();
            }
            delay
        };
        assert!(
            render(&mut app) > Duration::from_millis(16),
            "cued decks should be static"
        );
        app.status.players[0].play_state = 7;
        app.status.players[0].play_state_name = "CUE PLAY".into();
        assert!(
            render(&mut app) <= Duration::from_millis(16),
            "cue-play must animate in compare={compare}, focus={focus:?}"
        );
    }
}

#[test]
fn cued_and_paused_decks_draw_waveforms_without_a_beat_counter() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        for (state, name, beat) in [(5, "PAUSED", 0), (6, "CUED", u32::MAX)] {
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
            app.page = Page::Players;
            app.status.players[0].is_playing = false;
            app.status.players[0].play_state = state;
            app.status.players[0].play_state_name = name.into();
            app.status.players[0].beat_in_track = beat;
            let snapshot = app.deck_snapshot(1);
            assert!(snapshot.position.is_none());
            assert_eq!(snapshot.waveform_position, Some(0.0));
            assert!(!snapshot.assets.detail.is_empty());
            for focus in [false, true] {
                app.deck_focus = focus.then_some(1);
                for style in [
                    crate::waveform::Style::Rgb,
                    crate::waveform::Style::ThreeBand,
                ] {
                    app.waveform_style = style;
                    frame(&mut app, &ctx, size, vec![]);
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                            ..Default::default()
                        },
                        |ui| app.render(ui),
                    );
                    let label = if focus {
                        "Player detail waveform"
                    } else {
                        "Compare waveform 1"
                    };
                    let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
                    let columns: usize = output
                        .shapes
                        .iter()
                        .map(|shape| match &shape.shape {
                            Shape::Mesh(mesh) => mesh
                                .vertices
                                .iter()
                                .filter(|vertex| rect.contains(vertex.pos) && vertex.color.a() > 0)
                                .count(),
                            _ => 0,
                        })
                        .sum();
                    assert!(
                        columns > 50,
                        "{name} waveform missing in {label}: {columns} columns"
                    );
                    assert!(
                        !app.hits
                            .iter()
                            .any(|(name, _)| name == "Compare playhead 1")
                    );
                }
            }
            // Unknown timing for a different loaded track must not reuse a prior center.
            app.status.players[0].track_id = 2;
            assert_eq!(app.deck_snapshot(1).waveform_position, Some(0.0));
            app.status.players[0].track_id = 0;
            assert!(app.deck_snapshot(1).waveform_position.is_none());
        }
    }
}

#[test]
fn waveform_layout_aligns_clocks_caps_heights_and_returns_from_title_inspection() {
    for size in [
        vec2(800.0, 480.0),
        vec2(1024.0, 600.0),
        vec2(1280.0, 800.0),
        vec2(800.0, 1000.0),
        vec2(1920.0, 1080.0),
    ] {
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
        app.page = Page::Players;
        assert!(
            app.deck_compare,
            "fresh settings should open the waveform layout"
        );
        app.deck_pair = [2, 1];
        app.selected = Some(3);
        app.status.players[1] = Player {
            device_number: 2,
            track_id: 2,
            track_title: "Night Transit".into(),
            is_playing: false,
            is_master: false,
            on_air: false,
            play_state: 5,
            play_state_name: "PAUSED".into(),
            pitch_pct: -8.0,
            beat_in_track: 80,
            ..app.status.players[0].clone()
        };
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let rect = |name: &str| app.hits.iter().find(|(n, _)| n == name).unwrap().1;
        let first = rect("Compare waveform 2");
        let second = rect("Compare waveform 1");
        assert!(first.bottom() < second.top());
        assert_eq!(first.x_range(), second.x_range());
        assert!((84.0..=180.0).contains(&first.height()));
        let first_head = rect("Compare playhead 2");
        let second_head = rect("Compare playhead 1");
        assert_eq!(
            first_head.left(),
            second_head.left(),
            "different pitches must retain a shared reference line"
        );
        assert!((first_head.left() - first.left() - first.width() * 0.35).abs() < 0.01);
        let left = rect("Compare overview 2");
        let right = rect("Compare overview 1");
        assert!(left.top() > second.bottom());
        assert!(left.right() < right.left());
        assert_eq!(left.y_range(), right.y_range());
        assert!((48.0..=80.0).contains(&left.height()));
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 42.0));
        for (name, rect) in &app.hits {
            if name.starts_with("Compare ") || name == "Deck pair" || name == "Deck compare" {
                assert!(
                    bounds.contains_rect(*rect),
                    "{name} outside {size:?}: {rect:?}"
                );
            }
            if name.starts_with("Compare title") {
                assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
            }
        }
        tap(&mut app, &ctx, size, "Compare title 2");
        assert_eq!(app.page, Page::Detail);
        assert_eq!(app.detail_track.as_ref().unwrap().id, 2);
        assert_eq!(app.selected, Some(3));
        assert!(!requests.borrow().playing);
        tap(&mut app, &ctx, size, "Back");
        assert_eq!(app.page, Page::Players);
        assert!(app.deck_compare && app.deck_focus.is_none());
        assert_eq!(app.deck_pair, [2, 1]);
        tap(&mut app, &ctx, size, "Compare overview 2");
        assert_eq!(app.deck_focus, Some(2));
        tap(&mut app, &ctx, size, "Player overview");
        assert!(app.deck_focus.is_none() && app.deck_compare);
        assert!(requests.borrow().track.is_none());
    }
}

#[test]
fn waveform_titles_respect_external_identity_and_stale_timing_has_no_playhead() {
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
    let (controller, requests, _results) = Controller::test();
    app.preview = controller;
    app.page = Page::Players;
    app.deck_compare = true;
    app.status.players[0].external = true;
    app.status.players[0].source = "USB · player 2".into();
    tap(&mut app, &ctx, size, "Compare title 1");
    assert_eq!(
        app.page,
        Page::Players,
        "an external ID must not inspect its local namesake"
    );
    tap(&mut app, &ctx, size, "Compare title 2");
    assert_eq!(app.page, Page::Players, "an empty deck must not inspect");
    tap(&mut app, &ctx, size, "Compare player 1");
    assert_eq!(app.deck_focus, Some(1));
    tap(&mut app, &ctx, size, "Player overview");
    app.options.demo = false;
    app.last_status = Some(Instant::now() - Duration::from_secs(5));
    frame(&mut app, &ctx, size, vec![]);
    assert!(
        !app.hits
            .iter()
            .any(|(name, _)| name.starts_with("Compare playhead"))
    );
    assert!(requests.borrow().track.is_none());
    assert!(app.status.players[0].is_playing);
}

#[test]
fn title_taps_inspect_loaded_library_tracks_without_playback_or_external_id_collisions() {
    for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
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
        app.page = Page::Players;
        app.selected = Some(3);
        // The loaded track need not appear in the active playlist or filter.
        app.tracks = vec![
            app.all_tracks
                .iter()
                .find(|track| track.id == 3)
                .unwrap()
                .clone(),
        ];
        app.reindex();
        frame(&mut app, &ctx, size, vec![]);
        let title = app
            .hits
            .iter()
            .find(|(name, _)| name == "Deck title 1")
            .unwrap()
            .1;
        let heading = app
            .hits
            .iter()
            .find(|(name, _)| name == "Deck focus 1")
            .unwrap()
            .1;
        assert!(title.width() >= 48.0 && title.height() >= 48.0);
        assert!(heading.bottom() < title.top());
        tap(&mut app, &ctx, size, "Deck title 1");
        assert_eq!(app.page, Page::Detail);
        assert_eq!(app.detail_track.as_ref().unwrap().id, 1);
        assert_eq!(app.selected, Some(3));
        assert!(!requests.borrow().playing);
        assert!(app.status.players[0].is_playing);
        assert_eq!(app.status.players[0].track_id, 1);
        assert!(app.deck_focus.is_none());

        app.page = Page::Players;
        app.status.players[0].external = true;
        app.status.players[0].source = "USB · player 2".into();
        tap(&mut app, &ctx, size, "Deck title 1");
        assert_eq!(
            app.page,
            Page::Players,
            "external IDs must not alias library IDs"
        );
        app.status.players[0].external = false;
        app.status.players[0].source.clear();
        for id in [u32::MAX, 0] {
            app.status.players[0].track_id = id;
            tap(&mut app, &ctx, size, "Deck title 1");
            assert_eq!(
                app.page,
                Page::Players,
                "missing/empty track must not inspect"
            );
            assert!(app.deck_focus.is_none());
        }
        if size.y >= 800.0 {
            tap(&mut app, &ctx, size, "Deck title 4");
            assert_eq!(app.page, Page::Players, "empty card must not inspect");
            app.status.players.push(Player {
                device_number: 3,
                track_id: 3,
                track_title: "Third deck track".into(),
                ..app.status.players[1].clone()
            });
            tap(&mut app, &ctx, size, "Deck title 3");
            assert_eq!(app.page, Page::Detail);
            assert_eq!(app.detail_track.as_ref().unwrap().id, 3);
            assert!(!requests.borrow().playing);
        }
    }
}

#[test]
fn resizing_shows_four_cards_without_losing_the_pair_or_expanding_waveforms() {
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
    app.page = Page::Players;
    app.deck_pair = [3, 1];
    for (size, expected) in [
        (vec2(800.0, 480.0), 2),
        (vec2(1024.0, 600.0), 2),
        // Moving the view controls into the app header frees another 56 points.
        (vec2(800.0, 741.0), 2),
        (vec2(800.0, 742.0), 4),
        (vec2(1280.0, 800.0), 4),
        (vec2(800.0, 1000.0), 4),
        (vec2(1920.0, 1080.0), 4),
        (vec2(800.0, 480.0), 2),
    ] {
        // Let the scroll area's bounds settle after each native resize.
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.deck_pair, [3, 1]);
        let waves: Vec<_> = app
            .hits
            .iter()
            .filter(|(name, _)| name.starts_with("Deck waveform "))
            .collect();
        assert_eq!(waves.len(), expected, "wrong card count at {size:?}");
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for (_, wave) in &waves {
            assert!(
                bounds.contains_rect(*wave),
                "waveform outside {size:?}: {wave:?}"
            );
            assert!(
                (48.0..=96.0).contains(&wave.height()),
                "waveform stretched at {size:?}"
            );
        }
        let rect = |label: &str| app.hits.iter().find(|(name, _)| name == label).unwrap().1;
        let first = rect("Deck waveform 3");
        let second = rect("Deck waveform 1");
        assert_eq!(first.top(), second.top());
        assert!(first.right() < second.left());
        if expected == 4 {
            assert!(rect("Deck focus 2").top() > rect("Deck load 3").bottom());
            assert!(rect("Deck focus 4").top() > rect("Deck load 1").bottom());
        }
        for (name, control) in app
            .hits
            .iter()
            .filter(|(name, _)| name.starts_with("Deck load ") || name.starts_with("Deck inspect "))
        {
            assert!(
                bounds.contains_rect(*control),
                "{name} outside {size:?}: {control:?}"
            );
            assert!(control.height() >= 48.0 && control.width() >= 48.0);
        }
    }
    let large = vec2(1280.0, 800.0);
    tap(&mut app, &ctx, large, "Deck focus 4");
    assert_eq!(app.deck_focus, Some(4));
    assert!(requests.borrow().track.is_none());
    tap(&mut app, &ctx, large, "Player overview");
    tap(&mut app, &ctx, large, "Deck compare");
    frame(&mut app, &ctx, large, vec![]);
    let lanes: Vec<_> = app
        .hits
        .iter()
        .filter(|(name, _)| name.starts_with("Compare player "))
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(lanes, ["Compare player 3", "Compare player 1"]);
    tap(&mut app, &ctx, large, "Deck compare");
    frame(&mut app, &ctx, large, vec![]);
    assert_eq!(
        app.hits
            .iter()
            .filter(|(name, _)| name.starts_with("Deck waveform "))
            .count(),
        4
    );
}

#[test]
fn comparison_touch_flow_fits_both_screens_shares_zoom_and_returns_from_focus() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
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
        app.page = Page::Players;
        app.selected = Some(2);
        click(&mut app, &ctx, size, "Deck load 2");
        tap(&mut app, &ctx, size, "Deck compare");
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.deck_compare);
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for label in [
            "Compare player 1",
            "Compare player 2",
            "Compare waveform 1",
            "Compare waveform 2",
            "Compare zoom −",
            "Compare zoom +",
            "Deck compare",
            "Deck pair",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{label} outside screen: {rect:?}"
            );
        }
        tap(&mut app, &ctx, size, "Compare zoom +");
        assert_eq!(app.deck_span, 8.0);
        tap(&mut app, &ctx, size, "Compare player 2");
        assert_eq!(app.deck_focus, Some(2));
        assert!(requests.borrow().track.is_none());
        tap(&mut app, &ctx, size, "Player overview");
        assert!(app.deck_focus.is_none() && app.deck_compare);
        tap(&mut app, &ctx, size, "Deck pair");
        tap(&mut app, &ctx, size, "Pair 0 player 3");
        tap(&mut app, &ctx, size, "Pair 1 player 4");
        tap(&mut app, &ctx, size, "Pair apply");
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.hits.iter().any(|(n, _)| n == "Compare player 4"));
        tap(&mut app, &ctx, size, "Deck compare");
        frame(&mut app, &ctx, size, vec![]);
        assert!(!app.deck_compare);
        assert!(app.hits.iter().any(|(n, _)| n == "Deck load 4"));
    }
}

#[test]
fn custom_pairs_cancel_swap_and_restore_with_view_and_zoom_on_both_screens() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let root = std::env::temp_dir().join(format!(
            "vynull-player-pair-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("settings.json");
        let ctx = Context::default();
        let options = Options {
            demo: true,
            ..Default::default()
        };
        let mut app = crate::app::tests::cards_app(&ctx, options, Some(path.clone()));
        app.page = Page::Players;
        tap(&mut app, &ctx, size, "Deck compare");
        tap(&mut app, &ctx, size, "Compare zoom +");
        tap(&mut app, &ctx, size, "Deck pair");
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_size(Pos2::ZERO, size);
        for (label, rect) in app
            .hits
            .iter()
            .filter(|(label, _)| label.starts_with("Pair "))
        {
            assert!(
                bounds.contains_rect(*rect),
                "{label} outside screen: {rect:?}"
            );
            assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
        }
        tap(&mut app, &ctx, size, "Pair 0 player 2");
        assert_eq!(
            app.deck_picker,
            Some([1, 2]),
            "same player cannot fill both slots"
        );
        tap(&mut app, &ctx, size, "Pair 1 player 3");
        tap(&mut app, &ctx, size, "Pair swap");
        assert_eq!(app.deck_picker, Some([3, 1]));
        tap(&mut app, &ctx, size, "Pair apply");
        assert_eq!(app.deck_pair, [3, 1]);
        frame(&mut app, &ctx, size, vec![]);
        let top = app
            .hits
            .iter()
            .find(|(n, _)| n == "Compare player 3")
            .unwrap()
            .1;
        let bottom = app
            .hits
            .iter()
            .find(|(n, _)| n == "Compare player 1")
            .unwrap()
            .1;
        assert!(top.bottom() < bottom.top());

        for cancel in [true, false] {
            tap(&mut app, &ctx, size, "Deck pair");
            tap(&mut app, &ctx, size, "Pair 0 player 4");
            if cancel {
                tap(&mut app, &ctx, size, "Pair cancel");
            } else {
                frame(
                    &mut app,
                    &ctx,
                    size,
                    vec![eframe::egui::Event::Key {
                        key: eframe::egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
            }
            assert!(app.deck_picker.is_none());
            assert_eq!(app.deck_pair, [3, 1]);
            assert_eq!(settings::load(Some(&path)).players.pair, [3, 1]);
        }

        // Devices outside the initial four are selectable and remain selected
        // across a disconnect, reconnect, and application restart.
        app.status.players.push(Player {
            device_number: 6,
            ..app.status.players[0].clone()
        });
        tap(&mut app, &ctx, size, "Deck pair");
        tap(&mut app, &ctx, size, "Pair 0 player 6");
        tap(&mut app, &ctx, size, "Pair apply");
        app.status.players.retain(|p| p.device_number != 6);
        tap(&mut app, &ctx, size, "Compare player 6");
        tap(&mut app, &ctx, size, "Player zoom +");
        tap(&mut app, &ctx, size, "Player overview");
        app.connect(&ctx);
        assert_eq!(app.deck_pair, [6, 1]);
        assert!(app.deck_compare);
        assert_eq!(app.deck_span, 4.0);
        settings::save_zoom(&path, 32.0).unwrap();
        settings::save_audio(&path, None, 0.3).unwrap();

        let mut restored = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        restored.page = Page::Players;
        assert_eq!(restored.deck_pair, [6, 1]);
        assert!(restored.deck_compare);
        assert_eq!(restored.deck_span, 4.0);
        assert_eq!(restored.zoom, 32.0);
        assert_eq!(restored.preview.request.volume, 0.3);
        tap(&mut restored, &ctx, size, "Deck compare");
        frame(&mut restored, &ctx, size, vec![]);
        assert!(restored.hits.iter().any(|(n, _)| n == "Deck load 6"));
        assert!(restored.hits.iter().any(|(n, _)| n == "Deck load 1"));
        assert!(!settings::load(Some(&path)).players.compare);

        restored.settings_path = Some(path.join("not-a-directory.json"));
        tap(&mut restored, &ctx, size, "Deck compare");
        assert!(
            restored.deck_compare,
            "write failure must not block using the view"
        );
        assert!(restored.notice.contains("could not save"));
        assert!(!settings::load(Some(&path)).players.compare);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn focused_player_fits_switches_zoom_and_preserves_load_guards() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
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
        app.page = Page::Players;
        app.selected = Some(3);
        tap(&mut app, &ctx, size, "Deck focus 1");
        assert_eq!(app.deck_focus, Some(1));
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for label in [
            "Player overview",
            "Player select 4",
            "Player overview waveform",
            "Player detail waveform",
            "Player inspect",
            "Player load",
            "Player zoom −",
            "Player zoom +",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{label} outside screen: {rect:?}"
            );
            if !label.contains("waveform") {
                assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
            }
        }
        click(&mut app, &ctx, size, "Player load");
        assert_eq!(
            app.status.players[0].track_id, 1,
            "playing deck must wait for confirmation"
        );
        assert!(app.load_confirmation.is_some());
        click(&mut app, &ctx, size, "Load cancel");
        click(&mut app, &ctx, size, "Player detail waveform");
        assert!(
            requests.borrow().track.is_none(),
            "monitor must not start local preview"
        );
        click(&mut app, &ctx, size, "Player zoom +");
        assert_eq!(app.deck_span, 8.0);
        tap(&mut app, &ctx, size, "Player select 2");
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(app.deck_focus, Some(2));
        click(&mut app, &ctx, size, "Player load");
        assert_eq!(app.status.players[1].track_id, 3);
        let snapshot = app.deck_snapshot(2);
        assert!(!snapshot.assets.detail.is_empty());
        assert_eq!(snapshot.assets.sample_rate, 150);
        app.status.players[1].external = true;
        app.status.players[1].source = "USB · player 3".into();
        click(&mut app, &ctx, size, "Player inspect");
        assert_eq!(
            app.page,
            Page::Players,
            "external ID must not open a local track"
        );
        assert!(app.deck_snapshot(2).assets.detail.is_empty());
        app.linked = false;
        click(&mut app, &ctx, size, "Player load");
        assert!(app.status.players[1].external);
        assert!(app.deck_snapshot(2).position.is_none());
        tap(&mut app, &ctx, size, "Player select 4");
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.deck_snapshot(4).position.is_none());
        tap(&mut app, &ctx, size, "Player overview");
        assert!(app.deck_focus.is_none());
    }
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![eframe::egui::Event::PointerGone]);
    let pos = app
        .hits
        .iter()
        .find(|(n, _)| n == label)
        .unwrap()
        .1
        .center();
    for phase in [TouchPhase::Start, TouchPhase::End] {
        frame(
            app,
            ctx,
            size,
            vec![
                eframe::egui::Event::PointerMoved(pos),
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: phase == TouchPhase::Start,
                    modifiers: Modifiers::NONE,
                },
                eframe::egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos,
                    force: None,
                },
            ],
        );
    }
}

#[test]
fn disconnected_or_stale_player_loses_live_timing_and_track_changes_replace_assets() {
    let ctx = Context::default();
    let mut app = crate::app::tests::cards_app(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    assert!(app.deck_snapshot(1).position.is_some());
    app.status.peers.retain(|p| p.device_number != 1);
    assert!(app.deck_snapshot(1).position.is_none());
    app.status = crate::demo::status();
    app.options.demo = false;
    app.last_status = Some(Instant::now() - Duration::from_secs(20));
    assert!(app.deck_snapshot(1).position.is_none());
    app.options.demo = true;
    app.status.players[0].track_id = 2;
    assert_eq!(app.deck_snapshot(1).local.unwrap().id, 2);
    app.status.players[0].track_id = 0;
    let empty = app.deck_snapshot(1);
    assert!(empty.assets.detail.is_empty());
    assert!(empty.local.is_none());
}
