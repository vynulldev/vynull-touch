// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::frame;
use eframe::egui;

fn tap(app: &mut App, ctx: &Context, size: Vec2, name: &str) {
    frame(app, ctx, size, vec![]);
    frame(app, ctx, size, vec![]);
    let pos = app
        .hits
        .iter()
        .find(|(label, _)| label == name)
        .unwrap_or_else(|| panic!("missing {name}"))
        .1
        .center();
    tap_at(app, ctx, size, pos);
}

fn tap_at(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2) {
    for (phase, pressed) in [(TouchPhase::Start, true), (TouchPhase::End, false)] {
        frame(
            app,
            ctx,
            size,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos,
                    force: None,
                },
                egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
    frame(app, ctx, size, vec![]);
}

#[test]
fn grid_touch_editor_fits_nudges_aligns_and_edits_bpm_without_moving_cues_or_loop_times() {
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
        let (controller, requests, _) = Controller::test();
        app.preview = controller;
        let track = app.tracks[0].clone();
        app.inspect(track.clone());
        app.position = 1.11;
        let cues = app.detail.as_ref().unwrap().cues.clone();
        let region = crate::loops::Region {
            start: 1.0,
            end: 3.0,
        };
        app.preview.start_loop(region);
        frame(&mut app, &ctx, size, vec![]);
        frame(&mut app, &ctx, size, vec![]);
        let waveforms: Vec<_> = app
            .hits
            .iter()
            .filter(|(name, _)| matches!(name.as_str(), "Full waveform" | "Detail waveform"))
            .cloned()
            .collect();
        tap(&mut app, &ctx, size, "Edit grid");
        assert!(app.grid_editing());
        assert_eq!(
            app.hits
                .iter()
                .filter(|(name, _)| matches!(name.as_str(), "Full waveform" | "Detail waveform"))
                .cloned()
                .collect::<Vec<_>>(),
            waveforms
        );
        for name in [
            "Cue 1",
            "Cue 9",
            "Inspect load",
            "Edit cues",
            "Loop controls",
            "Inspect quantize",
            "Edit track",
            "Inspect playlists",
            "Edit grid",
        ] {
            assert!(
                !app.hits.iter().any(|(label, _)| label == name),
                "{name} must be replaced in edit mode"
            );
        }
        assert!(!requests.borrow().playing);
        let bounds = Rect::from_min_size(Pos2::ZERO, size - vec2(0.0, 26.0));
        for name in [
            "Full waveform",
            "Detail waveform",
            "Grid 0.1 ms",
            "Grid ‹ Earlier",
            "Grid Later ›",
            "Grid BPM",
            "Grid Apply BPM",
            "Grid Set beat 1 here",
            "Grid Reload grid",
            "Grid Done editing",
            "Grid Play",
            "Grid −",
            "Grid +",
        ] {
            let rect = app.hits.iter().find(|(label, _)| label == name).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{name} outside {size:?}: {rect:?}"
            );
            if !name.ends_with("waveform") {
                assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
            }
        }
        let rect = |name: &str| app.hits.iter().find(|(label, _)| label == name).unwrap().1;
        assert!(
            rect("Grid BPM").bottom() < rect("Grid Play").top(),
            "BPM row overlaps the fixed footer"
        );
        for row in [
            vec![
                "Grid 0.1 ms",
                "Grid 1 ms",
                "Grid 10 ms",
                "Grid 50 ms",
                "Grid ‹ Earlier",
                "Grid Later ›",
            ],
            vec![
                "Grid BPM",
                "Grid −0.01",
                "Grid +0.01",
                "Grid ÷2",
                "Grid ×2",
                "Grid Apply BPM",
            ],
            vec![
                "Grid Play",
                "Grid Set beat 1 here",
                "Grid Reload grid",
                "Grid −",
                "Grid +",
                "Grid Done editing",
            ],
        ] {
            for pair in row.windows(2) {
                assert!(
                    rect(pair[0]).right() < rect(pair[1]).left(),
                    "Overlapping controls: {pair:?}"
                );
            }
        }
        let before = app.detail.as_ref().unwrap().beat_grid.clone();
        tap(&mut app, &ctx, size, "Grid Later ›");
        assert!(
            (app.detail.as_ref().unwrap().beat_grid.beats()[0].time_ms
                - before.beats()[0].time_ms
                - 1.0)
                .abs()
                < 0.001
        );
        app.position = 1.11;
        tap(&mut app, &ctx, size, "Grid Set beat 1 here");
        let aligned = app.detail.as_ref().unwrap().beat_grid.clone();
        let beat = aligned
            .beats()
            .iter()
            .find(|b| (b.time_ms - 1110.0).abs() < 0.001)
            .unwrap();
        assert_eq!(beat.beat_in_bar, 1);
        assert_eq!(app.detail.as_ref().unwrap().cues, cues);
        assert_eq!(requests.borrow().loop_region, Some(region));
        assert!(!requests.borrow().playing);
        tap(&mut app, &ctx, size, "Grid BPM");
        for name in ["Grid 0", "Grid Backspace", "Grid Clear", "Grid Done"] {
            assert!(
                bounds.contains_rect(app.hits.iter().find(|(label, _)| label == name).unwrap().1)
            );
        }
        for name in ["Grid Clear", "Grid 6", "Grid 0", "Grid Done"] {
            tap(&mut app, &ctx, size, name);
        }
        assert!(app.detail.as_ref().unwrap().beat_grid.matches(&aligned)); // Entry is not a write.
        tap(&mut app, &ctx, size, "Grid Apply BPM");
        assert_eq!(app.detail_track.as_ref().unwrap().bpm, 60.0);
        let corrected = app.detail.as_ref().unwrap().beat_grid.clone();
        assert_eq!(corrected.beats()[0], aligned.beats()[0]);
        assert!(
            corrected
                .beats()
                .windows(2)
                .all(|w| (w[1].time_ms - w[0].time_ms - 1000.0).abs() < 0.001)
        );
        tap(&mut app, &ctx, size, "Grid Done editing");
        assert!(app.hits.iter().any(|(label, _)| label == "Cue 1"));
        assert_eq!(app.page, Page::Detail);
        assert!(!app.grid_editor.as_ref().unwrap().open);
        app.inspect(
            app.all_tracks
                .iter()
                .find(|t| t.id == track.id)
                .unwrap()
                .clone(),
        );
        assert!(app.detail.as_ref().unwrap().beat_grid.matches(&corrected));
        assert_eq!(app.detail.as_ref().unwrap().cues, cues);
    }
}

#[test]
fn grid_failures_block_relative_retries_until_reload_and_ignore_old_replies() {
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
    app.preview = Controller::test().0;
    app.inspect(app.tracks[0].clone());
    app.open_grid_editor();
    let snapshot = app.grid_editor.as_ref().unwrap().snapshot.clone().unwrap();
    let (worker, commands, events) = Worker::test();
    app.worker = Some(worker);
    app.options.demo = false;
    app.library_ready = true;
    app.library_session = Some("one".into());
    app.last_status = Some(Instant::now());
    tap(&mut app, &ctx, size, "Grid Later ›");
    let command = commands.try_recv().unwrap();
    assert!(matches!(
        command,
        Command::Grid {
            change: Some(Change::Nudge(1.0)),
            ..
        }
    ));
    tap(&mut app, &ctx, size, "Grid Later ›");
    assert!(commands.try_recv().is_err());
    for name in ["Back", "Grid Done editing", "Setup", "Players"] {
        tap(&mut app, &ctx, size, name);
        assert_eq!(app.page, Page::Detail);
        assert!(app.grid_editing());
        assert!(app.grid_pending());
    }
    events
        .send(Event::Grid(
            snapshot.track.id,
            app.token,
            true,
            Err("Reply lost; the edit may have applied".into()),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.grid_editor.as_ref().unwrap().needs_reload);
    assert!(app.detail.as_ref().unwrap().beat_grid.is_empty());
    tap(&mut app, &ctx, size, "Grid Later ›");
    assert!(commands.try_recv().is_err());
    tap(&mut app, &ctx, size, "Grid Reload grid");
    assert!(matches!(
        commands.try_recv().unwrap(),
        Command::Grid { change: None, .. }
    ));
    events
        .send(Event::Grid(
            snapshot.track.id,
            app.token + 1,
            false,
            Ok(Box::new(snapshot.clone())),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.grid_editor.as_ref().unwrap().pending);
    events
        .send(Event::Grid(
            snapshot.track.id,
            app.token,
            false,
            Ok(Box::new(snapshot.clone())),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.grid_editor.as_ref().unwrap().writable());
    assert!(
        app.detail
            .as_ref()
            .unwrap()
            .beat_grid
            .matches(&snapshot.grid)
    );
    tap(&mut app, &ctx, size, "Grid Later ›");
    assert!(commands.try_recv().is_ok());
    // Navigating to a different track invalidates the old command generation.
    app.options.demo = true;
    app.inspect(app.tracks[1].clone());
    let grid = app.detail.as_ref().unwrap().beat_grid.clone();
    events
        .send(Event::Grid(
            snapshot.track.id,
            app.token - 1,
            true,
            Ok(Box::new(snapshot)),
        ))
        .unwrap();
    frame(&mut app, &ctx, size, vec![]);
    assert!(app.grid_editor.is_none());
    assert!(app.detail.as_ref().unwrap().beat_grid.matches(&grid));
}

#[test]
fn locked_empty_and_invalid_bpm_keep_grid_editor_read_only() {
    for locked in [false, true] {
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
        app.preview = Controller::test().0;
        let mut track = app.tracks[0].clone();
        track.analysis_locked = locked;
        app.inspect(track);
        if !locked {
            app.detail.as_mut().unwrap().beat_grid = Default::default();
        }
        app.open_grid_editor();
        let before = app.detail.as_ref().unwrap().beat_grid.clone();
        assert!(!app.grid_editor.as_ref().unwrap().writable());
        for name in ["Grid Later ›", "Grid Set beat 1 here", "Grid Apply BPM"] {
            tap(&mut app, &ctx, size, name);
        }
        assert!(app.detail.as_ref().unwrap().beat_grid.matches(&before));
        assert!(!app.hits.iter().any(|(name, _)| name.starts_with("Cue ")));
        assert!(app.cue_editor.is_none() && app.cue_quick.is_none());
    }
    let mut editor = Editor::new();
    for invalid in ["", "NaN", "0", "401", "123..4"] {
        editor.bpm = invalid.into();
        assert!(editor.tempo().is_none());
    }
}

fn escape(app: &mut App, ctx: &Context, size: Vec2) {
    for pressed in [true, false] {
        frame(
            app,
            ctx,
            size,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
        );
    }
}

#[test]
fn inline_grid_mode_seeks_freely_and_restores_inspect_controls_and_quantize() {
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
    app.preview = Controller::test().0;
    let track = app.tracks[0].clone();
    app.inspect(track.clone());
    app.inspect_quantize = true;
    app.loop_controls = true;
    tap(&mut app, &ctx, size, "Edit grid");
    assert!(!app.hits.iter().any(|(name, _)| name == "Inspect quantize"));
    assert!(app.inspect_quantize);
    let grid = app.detail.as_ref().unwrap().beat_grid.clone();
    for name in ["Detail waveform", "Full waveform"] {
        let rect = app.hits.iter().find(|(label, _)| label == name).unwrap().1;
        let range = if name == "Detail waveform" {
            app.waveform_window(&track)
        } else {
            0.0..app.preview_duration(&track)
        };
        let pos = pos2(rect.left() + rect.width() * 0.371, rect.center().y);
        let expected = range.start
            + f64::from((pos.x - rect.left()) / rect.width()) * (range.end - range.start);
        tap_at(&mut app, &ctx, size, pos);
        assert!((app.position - expected).abs() < 0.0001);
        assert!(
            (app.position - grid.nearest(expected, f64::from(track.duration)).unwrap()).abs()
                > 0.001
        );
    }
    let zoom = app.zoom;
    tap(&mut app, &ctx, size, "Grid +");
    assert_eq!(app.zoom, zoom * 2.0);
    tap(&mut app, &ctx, size, "Grid −");
    assert_eq!(app.zoom, zoom);
    tap(&mut app, &ctx, size, "Grid BPM");
    assert!(app.grid_editor.as_ref().unwrap().typing);
    escape(&mut app, &ctx, size);
    assert!(app.grid_editing());
    assert!(!app.grid_editor.as_ref().unwrap().typing);
    escape(&mut app, &ctx, size);
    assert!(!app.grid_editing());
    assert_eq!(app.page, Page::Detail);
    assert!(app.loop_controls);
    assert!(app.inspect_quantize);
    assert_eq!(
        app.inspect_seek_position(1.111, f64::from(track.duration)),
        grid.nearest(1.111, f64::from(track.duration)).unwrap()
    );
    assert!(app.hits.iter().any(|(name, _)| name == "Cue 1"));
    assert!(app.detail.as_ref().unwrap().beat_grid.matches(&grid));
    tap(&mut app, &ctx, size, "Edit grid");
    tap(&mut app, &ctx, size, "Back");
    assert_eq!(app.page, Page::Detail);
    assert!(!app.grid_editing());
    tap(&mut app, &ctx, size, "Back");
    assert_ne!(app.page, Page::Detail);
    app.inspect(track);
    tap(&mut app, &ctx, size, "Edit grid");
    tap(&mut app, &ctx, size, "Playlists");
    assert_eq!(app.page, Page::Playlists);
    assert!(!app.grid_editor.as_ref().unwrap().open);
}
