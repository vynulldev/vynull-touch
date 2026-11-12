// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use eframe::egui::Event as InputEvent;

fn setup(path: Option<PathBuf>) -> (App, Context) {
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        path,
    );
    app.preview = Controller::test().0;
    app.status.players[0].key = "3A".into();
    app.status.players[0].bpm = 120.0;
    app.status.players[0].pitch_pct = 0.0;
    app.status.players[0].track_id = 99;
    for (track, key) in app.all_tracks.iter_mut().zip(["3A", "3B", "4A", "2A"]) {
        track.key = key.into();
        track.bpm = 120.0;
    }
    app.source(None);
    (app, ctx)
}

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
    let rect = app
        .hits
        .iter()
        .find(|(name, _)| name == label)
        .unwrap_or_else(|| panic!("Missing {label}"))
        .1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
        "{label}: {rect:?}"
    );
    assert!(rect.height() >= 48.0, "{label} touch height");
    for phase in [TouchPhase::Start, TouchPhase::End] {
        let pos = rect.center();
        let mut events = vec![
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
        ];
        if phase == TouchPhase::End {
            events.push(InputEvent::PointerGone);
        }
        frame(app, ctx, size, events);
    }
    for _ in 0..3 {
        frame(app, ctx, size, vec![]);
    }
}

fn has(app: &App, label: &str) -> bool {
    app.hits.iter().any(|(name, _)| name == label)
}

#[test]
fn key_matching_touch_controls_save_restore_and_survive_restart_and_reconnect() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        let root = std::env::temp_dir().join(format!(
            "vynull-key-matching-ui-{}-{}",
            std::process::id(),
            size.x
        ));
        let path = root.join("settings.json");
        settings::save_zoom(&path, 32.0).unwrap();
        let (mut app, ctx) = setup(Some(path.clone()));
        app.page = Page::Settings;
        tap(&mut app, &ctx, size, "Key matching");
        let bounds = Rect::from_min_size(Pos2::ZERO, size);
        for (name, rect) in &app.hits {
            if name.starts_with("Key rule") {
                assert!(bounds.contains_rect(*rect), "{name}: {rect:?}");
            }
        }
        tap(&mut app, &ctx, size, "Key rule Relative");
        tap(&mut app, &ctx, size, "Key rule Previous");
        let expected = KeyRules {
            relative: false,
            next: true,
            previous: false,
        };
        assert_eq!(app.matching.key_rules, expected);
        assert_eq!(settings::load(Some(&path)).key_matching, expected);
        assert_eq!(settings::load(Some(&path)).waveform_zoom, 32.0);
        tap(&mut app, &ctx, size, "Key rules done");
        assert!(app.key_matching_picker.is_none());
        assert_eq!(setup(Some(path.clone())).0.matching.key_rules, expected);
        app.connect(&ctx);
        assert_eq!(app.matching.key_rules, expected);
        tap(&mut app, &ctx, size, "Key matching");
        tap(&mut app, &ctx, size, "Key rules defaults");
        assert_eq!(app.matching.key_rules, KeyRules::default());
        assert_eq!(
            settings::load(Some(&path)).key_matching,
            KeyRules::default()
        );
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
        assert!(app.key_matching_picker.is_none());
        assert_eq!(app.page, Page::Settings);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn key_rules_apply_to_collection_playlists_and_match_results_without_changing_tempo_or_audio() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        for playlist in [false, true] {
            let (mut app, ctx) = setup(None);
            if playlist {
                let mut source = app.playlists[0].clone();
                source.track_ids = vec![1, 2, 3, 4];
                app.source(Some(source));
            }
            app.selected = Some(2);
            let visible = app.visible.clone();
            let audio_revision = app.preview.request.revision;
            tap(&mut app, &ctx, size, "Library match");
            tap(&mut app, &ctx, size, "Key matching");
            assert!(!app.library_match_picker);
            tap(&mut app, &ctx, size, "Key rule Relative");
            tap(&mut app, &ctx, size, "Key rule Previous");
            tap(&mut app, &ctx, size, "Key rules done");
            assert!(has(&app, "Key match 1") && has(&app, "Key match 3"));
            assert!(!has(&app, "Key match 2") && !has(&app, "Key match 4"));
            for id in 1..=4 {
                assert!(has(&app, &format!("BPM match {id}")));
            }
            assert_eq!(app.visible, visible);
            assert_eq!(app.selected, Some(2));
            assert_eq!(app.preview.request.revision, audio_revision);
            assert!(!app.load_dialog);
            app.open_matches();
            frame(&mut app, &ctx, size, vec![]);
            assert!(app.matching.results.iter().any(|m| m.id == 3));
            assert!(!app.matching.results.iter().any(|m| matches!(m.id, 2 | 4)));
            tap(&mut app, &ctx, size, "Key matching");
            tap(&mut app, &ctx, size, "Key rule Next");
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
            assert!(app.key_matching_picker.is_none());
            assert_eq!(app.page, Page::Match);
            assert!(app.matching.results.iter().any(|m| m.id == 1));
            assert!(!app.matching.results.iter().any(|m| matches!(m.id, 2..=4)));
            assert!(app.preview.request.track.is_none());
        }
    }
}

#[test]
fn key_matching_save_failure_keeps_previous_rules_and_reports_the_error() {
    let root =
        std::env::temp_dir().join(format!("vynull-key-rules-failure-{}", std::process::id()));
    std::fs::write(&root, "not a directory").unwrap();
    let (mut app, ctx) = setup(Some(root.join("settings.json")));
    app.page = Page::Settings;
    let size = vec2(800.0, 480.0);
    tap(&mut app, &ctx, size, "Key matching");
    tap(&mut app, &ctx, size, "Key rule Relative");
    assert_eq!(app.matching.key_rules, KeyRules::default());
    assert!(
        app.key_matching_picker
            .as_ref()
            .unwrap()
            .error
            .contains("Could not save")
    );
    let dialog = app
        .hits
        .iter()
        .find(|(name, _)| name == "Key rules dialog")
        .unwrap()
        .1;
    assert!(Rect::from_min_size(Pos2::ZERO, size).contains_rect(dialog));
    std::fs::remove_file(root).unwrap();
}
