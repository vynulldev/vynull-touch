// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use eframe::egui::Event as InputEvent;

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
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

fn app(ctx: &Context) -> App {
    App::new(
        ctx,
        Options {
            demo: true,
            ..Default::default()
        },
        None,
    )
}

#[test]
fn collection_and_playlist_preview_touch_controls_fit_and_only_play_explicitly() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0), vec2(1280.0, 800.0)] {
        for playlist in [false, true] {
            let ctx = Context::default();
            let mut app = app(&ctx);
            let (controller, requests, results) = Controller::test();
            app.preview = controller;
            app.preview.request.volume = 0.23;
            app.preview.request.device = Some(preview::OutputDevice {
                id: "test-headphones".into(),
                host: "ALSA".into(),
                name: "Headphones".into(),
            });
            if playlist {
                app.source(Some(app.playlists[0].clone()));
            }
            let source = app.source;
            let first = app.tracks[0].clone();
            let second = app.tracks[1].clone();
            tap(&mut app, &ctx, size, &format!("Track {}", first.id));
            assert!(
                requests.borrow().track.is_none(),
                "selecting must not start audio"
            );
            tap(&mut app, &ctx, size, "Library preview");
            let request = requests.borrow().clone();
            assert_eq!(request.track, Some((first.id, first.duration)));
            assert!(request.playing && request.activated);
            assert_eq!(request.volume, 0.23);
            assert_eq!(request.device.as_ref().unwrap().name, "Headphones");
            assert_eq!(app.page, Page::Collection);
            assert_eq!(app.source, source);
            assert!(!app.load_dialog && app.pending_load.is_none());
            results
                .send(preview::Snapshot {
                    epoch: request.epoch,
                    revision: request.revision,
                    phase: Phase::Playing,
                    position: 12.5,
                    duration: first.duration.into(),
                    ..Default::default()
                })
                .unwrap();
            frame(&mut app, &ctx, size, vec![]);
            assert_eq!(app.position, 12.5);
            assert!(app.library_preview_status().unwrap().contains("0:12"));
            tap(&mut app, &ctx, size, "Library preview");
            assert!(!requests.borrow().playing);
            tap(&mut app, &ctx, size, "Library preview");
            assert!(requests.borrow().playing);
            assert_eq!(requests.borrow().seconds, 12.5);
            tap(&mut app, &ctx, size, "Column BPM");
            assert!(
                requests.borrow().playing,
                "sorting retains the selected preview"
            );
            tap(&mut app, &ctx, size, "Column BPM");
            tap(&mut app, &ctx, size, "Column BPM");
            tap(&mut app, &ctx, size, &format!("Track {}", second.id));
            assert!(requests.borrow().track.is_none());
            assert!(app.library_preview.is_none());
            tap(&mut app, &ctx, size, "Library preview");
            assert_eq!(requests.borrow().track, Some((second.id, second.duration)));
            assert_eq!(requests.borrow().seconds, 0.0);
            // An old worker update cannot move the new track's cursor.
            results
                .send(preview::Snapshot {
                    epoch: request.epoch,
                    revision: request.revision,
                    phase: Phase::Playing,
                    position: 99.0,
                    ..Default::default()
                })
                .unwrap();
            frame(&mut app, &ctx, size, vec![]);
            assert_eq!(app.position, 0.0);
            tap(&mut app, &ctx, size, "Library load");
            assert!(app.load_dialog);
            assert!(
                !requests.borrow().playing,
                "opening the deck chooser pauses preview"
            );
        }
    }
}

#[test]
fn library_preview_stops_when_controls_or_selected_source_are_left() {
    let size = vec2(800.0, 480.0);
    for destination in [
        "Playlists",
        "Players",
        "Prepare",
        "Setup",
        "Search",
        "Inspect",
        "All tracks",
    ] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        app.source(Some(app.playlists[0].clone()));
        app.selected = Some(app.tracks[0].id);
        tap(&mut app, &ctx, size, "Library preview");
        assert!(requests.borrow().playing);
        tap(&mut app, &ctx, size, destination);
        assert!(!requests.borrow().playing, "{destination}");
        assert!(app.library_preview.is_none(), "{destination}");
        if destination == "Inspect" {
            assert_eq!(app.page, Page::Detail);
            tap(&mut app, &ctx, size, "Preview play");
            assert!(requests.borrow().playing);
            tap(&mut app, &ctx, size, "Back");
            assert!(requests.borrow().track.is_none());
        } else {
            assert!(requests.borrow().track.is_none(), "{destination}");
        }
    }
}

#[test]
fn library_preview_can_cancel_loading_retry_errors_and_restart_at_end() {
    let size = vec2(800.0, 480.0);
    let ctx = Context::default();
    let mut app = app(&ctx);
    let (controller, requests, results) = Controller::test();
    app.preview = controller;
    app.selected = Some(1);
    for phase in [Phase::Loading, Phase::Error, Phase::Ended] {
        tap(&mut app, &ctx, size, "Library preview");
        assert!(requests.borrow().playing);
        let request = requests.borrow().clone();
        results
            .send(preview::Snapshot {
                epoch: request.epoch,
                revision: request.revision,
                phase,
                position: 200.0,
                error: (phase == Phase::Error).then(|| "Output disconnected".into()),
                ..Default::default()
            })
            .unwrap();
        frame(&mut app, &ctx, size, vec![]);
        let message = app.library_preview_status().unwrap();
        match phase {
            Phase::Loading => assert!(message.contains("Tap Pause to cancel")),
            Phase::Error => assert!(message.contains("Output disconnected")),
            Phase::Ended => assert!(message.contains("tap Preview to restart")),
            _ => unreachable!(),
        }
        tap(&mut app, &ctx, size, "Library preview");
        if phase == Phase::Loading {
            assert!(!requests.borrow().playing && !requests.borrow().activated);
        } else {
            assert!(requests.borrow().playing && requests.borrow().activated);
            assert!(requests.borrow().revision > request.revision);
            if phase == Phase::Ended {
                assert_eq!(requests.borrow().seconds, 0.0);
                assert_eq!(app.position, 0.0);
            }
        }
        app.stop_library_preview();
    }
}

#[test]
fn library_preview_rejects_missing_or_replaced_tracks_and_connection_changes() {
    let size = vec2(800.0, 480.0);
    for change in [
        "file",
        "missing",
        "removed",
        "session",
        "server",
        "filter",
        "reconnect",
    ] {
        let ctx = Context::default();
        let mut app = app(&ctx);
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        app.selected = Some(1);
        tap(&mut app, &ctx, size, "Library preview");
        match change {
            "file" => app.tracks[0].file_path = "different-file.wav".into(),
            "missing" => app.tracks[0].file_missing = true,
            "removed" => {
                app.tracks.remove(0);
                app.reindex();
            }
            "session" => app.library_session = Some("new-library".into()),
            "server" => app.options.server = "http://other-server:9443".into(),
            "filter" => {
                app.query = "no matching tracks".into();
                app.filter();
            }
            "reconnect" => app.connect(&ctx),
            _ => unreachable!(),
        }
        frame(&mut app, &ctx, size, vec![]);
        assert!(requests.borrow().track.is_none(), "{change}");
        assert!(app.library_preview.is_none(), "{change}");
    }
    let ctx = Context::default();
    let mut app = app(&ctx);
    let (controller, requests, _results) = Controller::test();
    app.preview = controller;
    app.selected = Some(1);
    app.tracks[0].file_missing = true;
    tap(&mut app, &ctx, size, "Library preview");
    assert!(requests.borrow().track.is_none());
    app.tracks[0].file_missing = false;
    app.options.demo = false;
    app.library_session = None;
    tap(&mut app, &ctx, size, "Library preview");
    assert!(requests.borrow().track.is_none());
}
