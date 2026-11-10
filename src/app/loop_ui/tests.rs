// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};

#[test]
fn loop_controls_fit_resize_exit_save_and_recall_without_autoplay() {
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
        app.inspect(app.tracks[0].clone());
        app.position = 32.18;
        click(&mut app, &ctx, size, "Loop controls");
        frame(&mut app, &ctx, size, vec![]);
        let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
        for label in [
            "Full waveform",
            "Detail waveform",
            "Cue 8",
            "Loop 4",
            "Loop 32",
            "Loop ½",
            "Loop ×2",
            "Loop exit",
            "Loop save",
            "Preview play",
            "Loop controls",
        ] {
            let rect = app.hits.iter().find(|(n, _)| n == label).unwrap().1;
            assert!(
                bounds.contains_rect(rect),
                "{label} outside screen: {rect:?}"
            );
            assert!(rect.height() >= 48.0 && rect.width() >= 48.0);
        }
        click(&mut app, &ctx, size, "Loop 8");
        assert_eq!(
            requests.borrow().loop_region,
            Some(Region {
                start: 32.0,
                end: 36.0
            })
        );
        assert!(!requests.borrow().playing);
        click(&mut app, &ctx, size, "Preview play");
        app.position = 33.25;
        let seek = requests.borrow().seek;
        click(&mut app, &ctx, size, "Loop ×2");
        assert_eq!(
            requests.borrow().loop_region,
            Some(Region {
                start: 32.0,
                end: 40.0
            })
        );
        assert!(requests.borrow().playing);
        assert_eq!(
            requests.borrow().seek,
            seek,
            "resize uses the audio clock, not a stale UI seek"
        );
        assert_eq!(app.position, 33.25);
        click(&mut app, &ctx, size, "Loop ½");
        click(&mut app, &ctx, size, "Loop exit");
        assert!(requests.borrow().loop_region.is_none());
        assert!(requests.borrow().playing);
        assert_eq!(requests.borrow().seek, seek);
        app.position = 32.18;
        click(&mut app, &ctx, size, "Loop 4");
        click(&mut app, &ctx, size, "Loop save");
        assert!(!requests.borrow().playing);
        click(&mut app, &ctx, size, "Edit slot 8");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.kind, "loop");
        assert_eq!(app.cue_editor.as_ref().unwrap().draft.loop_ms, 34000);
        click(&mut app, &ctx, size, "Cue save");
        assert!(app.cue_editor.is_none());
        app.inspect(app.tracks[1].clone());
        app.inspect(app.tracks[0].clone());
        assert!(requests.borrow().loop_region.is_none());
        click(&mut app, &ctx, size, "Cue 8");
        assert_eq!(
            requests.borrow().loop_region,
            Some(Region {
                start: 32.0,
                end: 34.0
            })
        );
        assert!(!requests.borrow().playing);
        assert!(app.loop_controls);
        click(&mut app, &ctx, size, "Cue 2");
        assert!(requests.borrow().loop_region.is_none());
        click(&mut app, &ctx, size, "Cue 8");
        click(&mut app, &ctx, size, "Full waveform");
        assert!(
            requests.borrow().loop_region.is_none(),
            "seeking outside the loop exits it"
        );
        click(&mut app, &ctx, size, "Cue 8");
        click(&mut app, &ctx, size, "Back");
        assert!(requests.borrow().loop_region.is_none());
    }
}

#[test]
fn unavailable_grid_and_track_end_do_not_create_or_truncate_loops() {
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
    app.inspect(app.tracks[0].clone());
    app.position = f64::from(app.tracks[0].duration) - 0.1;
    click(&mut app, &ctx, size, "Loop controls");
    click(&mut app, &ctx, size, "Loop 32");
    assert!(app.preview.request.loop_region.is_none());
    assert!(app.loop_error.contains("Not enough beats"));
    app.detail.as_mut().unwrap().beat_grid = Default::default();
    frame(&mut app, &ctx, size, vec![]);
    click(&mut app, &ctx, size, "Loop 4");
    assert!(app.preview.request.loop_region.is_none());
}
