// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

fn settings_file(name: &str) -> PathBuf {
    std::env::temp_dir()
        .join(format!("vynull-connection-{name}-{}", std::process::id()))
        .join("settings.json")
}

#[test]
fn waveform_style_choices_fit_touch_setup_apply_and_restore_without_resetting_players() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let path = settings_file(&format!("waveform-{}", size.x));
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        let (controller, requests, _) = Controller::test();
        app.preview = controller;
        app.page = Page::Settings;
        app.deck_pair = [3, 1];
        app.deck_span = 8.0;
        app.selected = Some(3);
        frame(&mut app, &ctx, size, vec![]);
        frame(
            &mut app,
            &ctx,
            size,
            vec![
                eframe::egui::Event::PointerMoved(pos2(600.0, 350.0)),
                eframe::egui::Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: vec2(0.0, -180.0),
                    phase: TouchPhase::Move,
                    source: MouseWheelSource::Unknown,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
        for _ in 0..20 {
            frame(&mut app, &ctx, size, vec![]);
        }
        use crate::waveform::{Overview, Style};
        for shape in [Overview::Half, Overview::Full] {
            let label = format!("Waveform overview {}", shape.label());
            let rect = app.hits.iter().find(|(n, _)| n == &label).unwrap().1;
            assert!(
                rect.top() >= 68.0 && rect.bottom() <= size.y - 42.0,
                "{rect:?}"
            );
            assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
            click(&mut app, &ctx, size, &label);
            assert_eq!(app.waveform_overview, shape);
            assert_eq!(settings::load(Some(&path)).waveform_overview, shape);
        }
        for style in crate::waveform::Style::ALL {
            let label = format!("Waveform style {}", style.label());
            let rect = app.hits.iter().find(|(n, _)| n == &label).unwrap().1;
            assert!(
                rect.top() >= 68.0 && rect.bottom() <= size.y - 42.0,
                "{rect:?}"
            );
            assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
            click(&mut app, &ctx, size, &label);
            assert_eq!(app.waveform_style, style);
            assert_eq!(settings::load(Some(&path)).waveform_style, style);
            if style == Style::ThreeBand {
                // Neither disabled choice can overwrite the RGB/Blue preference.
                click(&mut app, &ctx, size, "Waveform overview Half");
                click(&mut app, &ctx, size, "Waveform overview Full");
                assert_eq!(app.waveform_overview, Overview::Full);
            } else {
                click(&mut app, &ctx, size, "Waveform overview Half");
                assert_eq!(app.waveform_overview, Overview::Half);
                click(&mut app, &ctx, size, "Waveform overview Full");
                assert_eq!(app.waveform_overview, Overview::Full);
            }
            assert_eq!(
                settings::load(Some(&path)).waveform_overview,
                Overview::Full
            );
            assert_eq!(app.deck_pair, [3, 1]);
            assert_eq!(app.deck_span, 8.0);
            assert_eq!(app.selected, Some(3));
            assert!(requests.borrow().track.is_none());
        }
        click(&mut app, &ctx, size, "Waveform overview Half");
        let restored = App::new(
            &Context::default(),
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        assert_eq!(restored.waveform_style, crate::waveform::Style::Blue);
        assert_eq!(restored.waveform_overview, Overview::Half);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

#[test]
fn startup_preferences_survive_demo_and_launcher_options_override_without_replacing_them() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        let path = settings_file(&format!("startup-{}", size.x));
        settings::save_server(&path, "http://pi.local:9443").unwrap();
        settings::save_zoom(&path, 32.0).unwrap();
        settings::save_startup_page(&path, StartupPage::Collection).unwrap();
        let ctx = Context::default();
        let mut app = App::new(
            &ctx,
            Options {
                demo: true,
                ..Default::default()
            },
            Some(path.clone()),
        );
        let (controller, _requests, _results) = Controller::test();
        app.preview = controller;
        assert_eq!(app.options.server, "http://pi.local:9443");
        for page in [
            StartupPage::Players,
            StartupPage::Playlists,
            StartupPage::Collection,
        ] {
            click(&mut app, &ctx, size, "Setup");
            click(&mut app, &ctx, size, &format!("Startup {}", page.label()));
            assert_eq!(
                app.page,
                Page::Settings,
                "choosing next startup must not navigate now"
            );
            let restarted = App::new(
                &Context::default(),
                Options {
                    demo: true,
                    ..Default::default()
                },
                Some(path.clone()),
            );
            assert_eq!(restarted.page, Page::from(page));
            assert_eq!(restarted.zoom, 32.0);
            assert_eq!(restarted.server_draft, "http://pi.local:9443");
        }
        settings::save_startup_page(&path, StartupPage::Playlists).unwrap();
        let explicit = App::new(
            &Context::default(),
            Options {
                demo: true,
                server: Options::default().server,
                server_given: true,
                startup_page: Some(StartupPage::Players),
                ..Default::default()
            },
            Some(path.clone()),
        );
        assert_eq!(
            explicit.options.server, "http://127.0.0.1:9443",
            "even an explicit default URL overrides the saved address"
        );
        assert_eq!(explicit.page, Page::Players);
        let saved = settings::load(Some(&path));
        assert_eq!(saved.startup_page, StartupPage::Playlists);
        assert_eq!(saved.server.as_deref(), Some("http://pi.local:9443"));
        click(&mut app, &ctx, size, "Try demo");
        assert_eq!(settings::load(Some(&path)).server, saved.server);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}

#[test]
fn only_a_successful_real_library_connection_remembers_the_server() {
    let path = settings_file("success");
    settings::save_server(&path, "http://old.local:9443").unwrap();
    settings::save_startup_page(&path, StartupPage::Players).unwrap();
    let ctx = Context::default();
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        Some(path.clone()),
    );
    let (worker, requests, events) = Worker::test();
    app.options.demo = false;
    app.options.server = "http://new.local:9443".into();
    app.worker = Some(worker);
    app.library_ready = false;
    events
        .send(Event::Library(Err("Cannot reach Vynull".into())))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(
        settings::load(Some(&path)).server.as_deref(),
        Some("http://old.local:9443")
    );
    assert!(
        !app.send(Command::Detail(1, 1)),
        "do not queue work behind an initial connection retry"
    );
    assert!(requests.try_recv().is_err());
    events
        .send(Event::Library(Ok(api::LibrarySnapshot {
            tracks: vec![],
            playlists: vec![],
            session: Some("one".into()),
            revision: None,
        })))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(
        settings::load(Some(&path)).server.as_deref(),
        Some("http://new.local:9443")
    );
    assert_eq!(
        settings::load(Some(&path)).startup_page,
        StartupPage::Players
    );
    app.options.demo = true;
    app.options.server = "http://demo-must-not-save.local".into();
    app.remember_server();
    assert_eq!(
        settings::load(Some(&path)).server.as_deref(),
        Some("http://new.local:9443")
    );
    std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
}

#[test]
fn outage_keeps_browsing_and_drafts_and_recovery_does_not_replay_writes() {
    let ctx = Context::default();
    let size = vec2(800.0, 480.0);
    let mut app = App::new(
        &ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    );
    let (controller, _requests, _results) = Controller::test();
    app.preview = controller;
    let playlist = app.playlists[0].clone();
    app.source(Some(playlist));
    app.query = "Mira".into();
    app.filters.min_bpm = Some(124.0);
    app.filter();
    app.selected = Some(5);
    app.inspect(app.all_tracks[4].clone());
    app.open_cue_editor();
    app.cue_editor.as_mut().unwrap().draft.label = "Keep this draft".into();
    let draft = app.cue_editor.as_ref().unwrap().draft.clone();
    app.options.demo = false;
    app.library_session = Some("one".into());
    let (worker, requests, events) = Worker::test();
    app.worker = Some(worker);
    events
        .send(Event::Status(
            Instant::now(),
            Err("Cannot reach Vynull".into()),
        ))
        .unwrap();
    events
        .send(Event::Connection(api::Connection::Waiting {
            attempt: 3,
            until: Instant::now() + Duration::from_secs(4),
        }))
        .unwrap();
    app.poll(&ctx);
    assert!(!app.fresh());
    assert_eq!(app.cue_editor.as_ref().unwrap().draft, draft);
    // The banner remains usable when the user closes their draft explicitly.
    app.cue_editor = None;
    frame(&mut app, &ctx, size, vec![]);
    let settings = app.hits.iter().find(|(name, _)| name == "Setup").unwrap().1;
    assert!(
        Rect::from_min_size(Pos2::ZERO, size - vec2(0.0, 26.0)).contains_rect(settings),
        "recovery must leave Settings and its diagnostics accessible"
    );
    click(&mut app, &ctx, size, "Connection retry");
    assert!(
        requests.try_recv().is_err(),
        "retry must not enqueue a load or cue edit"
    );
    events
        .send(Event::Status(Instant::now(), Ok((demo::status(), true))))
        .unwrap();
    app.poll(&ctx);
    assert!(app.fresh());
    assert_eq!(app.page, Page::Detail);
    assert_eq!(app.selected, Some(5));
    assert_eq!(app.query, "Mira");
    assert_eq!(app.source, Some(1));
    assert_eq!(app.filters.min_bpm, Some(124.0));
    assert!(requests.try_recv().is_err());
    frame(&mut app, &ctx, size, vec![]);
    assert!(!app.hits.iter().any(|(name, _)| name == "Connection retry"));
}

#[test]
fn changed_session_discards_old_ids_drafts_and_pending_loads() {
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
    app.selected = Some(1);
    app.inspect(app.tracks[0].clone());
    app.open_cue_editor();
    app.cue_editor.as_mut().unwrap().pending = true;
    app.load_busy = true;
    app.library_session = Some("old".into());
    let (worker, commands, events) = Worker::test();
    app.worker = Some(worker);
    // Demo reconnection keeps this test off the network while exercising reset.
    events
        .send(Event::Status(
            Instant::now(),
            Err("Library changed; reconnect to reload its tracks".into()),
        ))
        .unwrap();
    // A queued old success must not restore state after the reset.
    events.send(Event::Load(Ok((1, 2)))).unwrap();
    app.poll(&ctx);
    assert_eq!(app.page, Page::Players);
    assert!(app.selected.is_none());
    assert!(app.detail_track.is_none());
    assert!(app.cue_editor.is_none());
    assert!(!app.load_busy);
    assert!(app.pending_load.is_none());
    assert!(app.library_session.is_none());
    assert!(app.notice.contains("outcome is unknown"));
    assert!(requests.borrow().track.is_none());
    assert!(commands.try_recv().is_err());
}
