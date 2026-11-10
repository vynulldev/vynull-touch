// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

fn tap(app: &mut App, ctx: &Context, size: Vec2, label: &str) {
    frame(app, ctx, size, vec![]);
    let rect = app.hits.iter().find(|(name, _)| name == label).unwrap().1;
    let pos = rect.center();
    for (phase, pressed) in [(TouchPhase::Start, true), (TouchPhase::End, false)] {
        frame(
            app,
            ctx,
            size,
            vec![
                eframe::egui::Event::PointerMoved(pos),
                eframe::egui::Event::Touch {
                    device_id: TouchDeviceId(1),
                    id: TouchId(1),
                    phase,
                    pos,
                    force: None,
                },
                eframe::egui::Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn touch_beat_jumps_fit_and_preserve_transport_phase_and_loop_seek_behavior() {
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
        let (controller, requests, results) = Controller::test();
        app.preview = controller;
        app.inspect(app.tracks[0].clone());
        app.position = 32.125;
        app.show_grid = false;
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for name in [
            "Full waveform",
            "Detail waveform",
            "Cue 8",
            "Beat jump back",
            "Beat jump size",
            "Beat jump forward",
            "Grid",
            "Fit waveform",
            "Zoom in",
            "Zoom out",
            "Preview play",
            "Inspect load",
            "Loop controls",
        ] {
            let rect = app.hits.iter().find(|(label, _)| label == name).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{name} outside screen at {size:?}: {rect:?}"
            );
            assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
        }
        tap(&mut app, &ctx, size, "Beat jump forward");
        assert_eq!(app.position, 34.125);
        assert_eq!(requests.borrow().seconds, 34.125);
        assert!(!requests.borrow().playing);
        assert!(
            !requests.borrow().activated,
            "A paused jump must not activate audio"
        );
        tap(&mut app, &ctx, size, "Beat jump back");
        assert_eq!(app.position, 32.125);
        click(&mut app, &ctx, size, "Preview play");
        let stale = requests.borrow().clone();
        app.preview.request.loop_region = Some(crate::loops::Region {
            start: 32.0,
            end: 36.0,
        });
        tap(&mut app, &ctx, size, "Beat jump forward");
        assert!(requests.borrow().playing);
        assert!(requests.borrow().loop_region.is_some());
        tap(&mut app, &ctx, size, "Beat jump size");
        frame(&mut app, &ctx, size, vec![]);
        for beats in settings::BEAT_JUMP_SIZES {
            let rect = app
                .hits
                .iter()
                .find(|(label, _)| label == &format!("Beat jump size {beats}"))
                .unwrap()
                .1;
            assert!(
                bounds.contains_rect(rect),
                "Size {beats} outside screen: {rect:?}"
            );
            assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
        }
        tap(&mut app, &ctx, size, "Beat jump size 16");
        assert_eq!(app.beat_jump_beats, 16);
        tap(&mut app, &ctx, size, "Beat jump forward");
        assert_eq!(app.position, 42.125);
        assert!(requests.borrow().playing);
        assert!(
            requests.borrow().loop_region.is_none(),
            "Jumping outside a loop exits it"
        );
        results
            .send(preview::Snapshot {
                epoch: stale.epoch,
                revision: stale.revision,
                phase: Phase::Playing,
                position: 32.25,
                ..Default::default()
            })
            .unwrap();
        frame(&mut app, &ctx, size, vec![]);
        assert_eq!(
            app.position, 42.125,
            "A stale audio reply cannot undo the jump"
        );
        app.inspect(app.tracks[1].clone());
        assert_eq!(app.beat_jump_beats, 16);
        assert!(!requests.borrow().playing);
    }
}

#[test]
fn beat_jump_limits_missing_analysis_and_escape_leave_inspection_usable() {
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
    let (controller, requests, _results) = Controller::test();
    app.preview = controller;
    app.inspect(app.tracks[0].clone());
    let duration = f64::from(app.tracks[0].duration);
    let seek = requests.borrow().seek;
    tap(&mut app, &ctx, size, "Beat jump back");
    assert_eq!(requests.borrow().seek, seek);
    app.position = duration - 0.01;
    tap(&mut app, &ctx, size, "Beat jump forward");
    assert_eq!(app.position, duration);
    let seek = requests.borrow().seek;
    tap(&mut app, &ctx, size, "Beat jump forward");
    assert_eq!(requests.borrow().seek, seek);
    app.detail.as_mut().unwrap().beat_grid = Default::default();
    tap(&mut app, &ctx, size, "Beat jump back");
    assert_eq!(requests.borrow().seek, seek);
    app.detail = None;
    tap(&mut app, &ctx, size, "Beat jump back");
    assert_eq!(requests.borrow().seek, seek);
    tap(&mut app, &ctx, size, "Beat jump size");
    assert!(Popup::is_any_open(&ctx));
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
    assert!(!Popup::is_any_open(&ctx));
    assert_eq!(
        app.page,
        Page::Detail,
        "Escape should dismiss the size menu first"
    );
}
