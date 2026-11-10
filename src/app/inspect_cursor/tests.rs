// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

fn pointer(app: &mut App, ctx: &Context, size: Vec2, pos: Pos2, phase: TouchPhase) {
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

fn rect(app: &App, name: &str) -> Rect {
    app.hits.iter().find(|(label, _)| label == name).unwrap().1
}

#[test]
fn cursor_beat_stays_on_downbeat_after_audio_frame_rounding_and_cue_recall() {
    use crate::beat_grid::{Beat, BeatGrid, TrackAnalysis};
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
    let (controller, requests, results) = Controller::test();
    app.preview = controller;
    app.inspect(app.tracks[0].clone());
    app.detail.as_mut().unwrap().beat_grid = BeatGrid::new(
        TrackAnalysis {
            track_id: 1,
            beats: (0..12)
                .map(|i| Beat {
                    time_ms: 39_000.2 + f64::from(i) * 500.05,
                    beat_in_bar: (i % 4 + 1) as u8,
                })
                .collect(),
        },
        1,
    )
    .unwrap();
    app.inspect_quantize = true;
    let target = app.inspect_seek_position(41.0, 284.0);
    app.position = target;
    assert!(app.inspect_cursor_label().starts_with("Beat 5 (1/4)"));
    for rate in [8_000, 44_100, 48_000, 96_000, 192_000] {
        app.preview.seek(target, false);
        // The audio engine reports complete frames, flooring fractional frames.
        let position = (target * f64::from(rate)).floor() / f64::from(rate);
        let request = requests.borrow().clone();
        for phase in [Phase::Paused, Phase::Playing, Phase::Buffering] {
            results
                .send(preview::Snapshot {
                    epoch: request.epoch,
                    revision: request.revision,
                    phase,
                    position,
                    ..Default::default()
                })
                .unwrap();
            frame(&mut app, &ctx, size, vec![]);
            assert!(
                app.inspect_cursor_label().starts_with("Beat 5 (1/4)"),
                "{rate} Hz, {phase:?}: {}",
                app.inspect_cursor_label()
            );
            assert_eq!(
                app.position, position,
                "the readout must not move the cursor"
            );
        }
    }
    // A snapped cue is persisted to whole milliseconds, then recalled exactly.
    app.detail.as_mut().unwrap().cues[0].time_ms = 41_000;
    click(&mut app, &ctx, size, "Cue 1");
    assert_eq!(requests.borrow().seconds, 41.0);
    assert!(app.inspect_cursor_label().starts_with("Beat 5 (1/4)"));
    click(&mut app, &ctx, size, "Beat jump forward");
    assert!(app.inspect_cursor_label().starts_with("Beat 9 (1/4)"));
    click(&mut app, &ctx, size, "Beat jump back");
    assert!(app.inspect_cursor_label().starts_with("Beat 5 (1/4)"));
    app.position = target - 0.002;
    assert!(
        app.inspect_cursor_label().starts_with("Beat 4 (4/4)"),
        "a position genuinely before the downbeat must retain the previous beat"
    );
}

#[test]
fn quantized_waveform_touch_and_jumps_preserve_transport_but_saved_cues_stay_exact() {
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
        let (controller, requests, _results) = Controller::test();
        app.preview = controller;
        app.inspect(app.tracks[0].clone());
        app.position = 32.125;
        app.show_grid = false;
        click(&mut app, &ctx, size, "Inspect quantize");
        assert!(app.inspect_quantize);
        assert_eq!(app.position, 32.125, "Enabling Quantize must not seek");
        let overview = rect(&app, "Full waveform");
        let target = pos2(
            overview.left() + overview.width() * 0.1135,
            overview.center().y,
        );
        pointer(&mut app, &ctx, size, target, TouchPhase::Start);
        pointer(&mut app, &ctx, size, target, TouchPhase::End);
        assert_eq!(app.position, 32.0);
        assert!(!requests.borrow().playing);
        assert!(!requests.borrow().activated);
        click(&mut app, &ctx, size, "Preview play");
        let detail = rect(&app, "Detail waveform");
        let window = app.waveform_window(app.detail_track.as_ref().unwrap());
        pointer(&mut app, &ctx, size, detail.center(), TouchPhase::Start);
        let destination = pos2(detail.left() + detail.width() * 0.6243, detail.center().y);
        pointer(&mut app, &ctx, size, destination, TouchPhase::Move);
        assert_eq!(app.position, 36.5);
        assert_eq!(
            app.waveform_window(app.detail_track.as_ref().unwrap()),
            window
        );
        assert!(requests.borrow().playing && requests.borrow().dragging);
        pointer(&mut app, &ctx, size, destination, TouchPhase::End);
        assert_eq!(requests.borrow().seconds, 36.5);
        assert!(requests.borrow().playing && !requests.borrow().dragging);
        app.detail
            .as_mut()
            .unwrap()
            .cues
            .iter_mut()
            .find(|cue| cue.number == 2)
            .unwrap()
            .time_ms = 32123;
        click(&mut app, &ctx, size, "Cue 2");
        assert_eq!(app.position, 32.123);
        click(&mut app, &ctx, size, "Beat jump forward");
        assert_eq!(app.position, 34.0);
        assert!(requests.borrow().playing);
        click(&mut app, &ctx, size, "Beat jump back");
        assert_eq!(app.position, 32.0);
        app.detail.as_mut().unwrap().beat_grid = Default::default();
        click(&mut app, &ctx, size, "Inspect quantize");
        assert!(
            app.inspect_quantize,
            "An unavailable grid must retain the preference"
        );
        pointer(&mut app, &ctx, size, target, TouchPhase::Start);
        pointer(&mut app, &ctx, size, target, TouchPhase::End);
        assert!((app.position - 32.234).abs() < 0.001);
        assert!(app.inspect_cursor_label().starts_with("No beat grid"));
        app.inspect(app.tracks[0].clone());
        assert!(app.inspect_quantize);
        click(&mut app, &ctx, size, "Inspect quantize");
        assert!(!app.inspect_quantize);
        pointer(&mut app, &ctx, size, target, TouchPhase::Start);
        pointer(&mut app, &ctx, size, target, TouchPhase::End);
        assert!((app.position - 32.234).abs() < 0.001);
    }
}

#[test]
fn cursor_readout_is_precise_clipped_and_available_without_waveform_or_grid() {
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
        let (controller, _requests, _results) = Controller::test();
        app.preview = controller;
        app.inspect(app.tracks[0].clone());
        app.show_grid = false;
        app.position = 32.125;
        assert_eq!(app.inspect_cursor_label(), "Beat 65 (1/4) · 0:32.125");
        app.position = 59.9996;
        assert!(app.inspect_cursor_label().ends_with("1:00.000"));
        for position in [0.0, 32.125, f64::from(app.tracks[0].duration)] {
            app.position = position;
            frame(&mut app, &ctx, size, vec![]);
            assert!(
                rect(&app, "Detail waveform").contains_rect(rect(&app, "Inspect cursor label"))
            );
            let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
            for name in [
                "Inspect quantize",
                "Beat jump forward",
                "Zoom in",
                "Inspect load",
                "Preview play",
            ] {
                assert!(
                    bounds.contains_rect(rect(&app, name)),
                    "{name} must remain visible"
                );
            }
        }
        app.detail = None;
        frame(&mut app, &ctx, size, vec![]);
        assert!(app.inspect_cursor_label().starts_with("Beat unavailable"));
        assert!(rect(&app, "Detail waveform").contains_rect(rect(&app, "Inspect cursor label")));
    }
}
