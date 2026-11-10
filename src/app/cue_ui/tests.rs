// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::app::tests::{click, frame};
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
    let (preview, _, _) = Controller::test();
    app.preview = preview;
    app.inspect(app.tracks[0].clone());
    (app, ctx)
}

fn input(pos: Pos2, phase: TouchPhase, touch: bool) -> Vec<InputEvent> {
    let mut events = vec![InputEvent::PointerMoved(pos)];
    if matches!(phase, TouchPhase::Start | TouchPhase::End) {
        events.push(InputEvent::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: phase == TouchPhase::Start,
            modifiers: Modifiers::NONE,
        });
    }
    if touch {
        events.push(InputEvent::Touch {
            device_id: TouchDeviceId(1),
            id: TouchId(1),
            phase,
            pos,
            force: None,
        });
        // winit removes the emulated pointer at the end of a real touch.
        if matches!(phase, TouchPhase::End | TouchPhase::Cancel) {
            events.push(InputEvent::PointerGone);
        }
    }
    events
}

fn advance(app: &mut App, ctx: &Context, size: Vec2, seconds: f64, events: Vec<InputEvent>) {
    let input = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(ctx.input(|i| i.time) + seconds),
        events,
        ..Default::default()
    };
    let mut output = ctx.run_ui(input, |ui| app.render(ui));
    output.textures_delta.clear();
}

fn pad(app: &mut App, ctx: &Context, size: Vec2, number: u16) -> Rect {
    frame(app, ctx, size, vec![]);
    app.hits
        .iter()
        .find(|(name, _)| *name == format!("Cue {number}"))
        .unwrap()
        .1
}

#[test]
fn eight_pads_fit_below_detail_and_taps_create_snapped_cues_without_interrupting_preview() {
    for size in [vec2(800.0, 480.0), vec2(1024.0, 600.0)] {
        for touch in [false, true] {
            let (mut app, ctx) = setup();
            let bounds = Rect::from_min_max(Pos2::ZERO, pos2(size.x, size.y - 26.0));
            let mut row_y = None;
            for number in 1..=8 {
                let rect = pad(&mut app, &ctx, size, number);
                assert!(bounds.contains_rect(rect));
                assert!(rect.width() >= 48.0 && rect.height() >= 48.0);
                assert_eq!(*row_y.get_or_insert(rect.top()), rect.top());
                let waveform = app
                    .hits
                    .iter()
                    .find(|(n, _)| n == "Detail waveform")
                    .unwrap()
                    .1;
                assert!((1.9..=6.1).contains(&(rect.top() - waveform.bottom())));
            }
            app.position = 32.18;
            app.preview.toggle(app.position);
            app.show_grid = false; // Hiding grid lines must not turn snapping off.
            let before = app.preview.request.clone();
            let pos = pad(&mut app, &ctx, size, 5).center();
            frame(&mut app, &ctx, size, vec![InputEvent::PointerMoved(pos)]);
            frame(&mut app, &ctx, size, input(pos, TouchPhase::Start, touch));
            frame(&mut app, &ctx, size, input(pos, TouchPhase::End, touch));
            let saved = app
                .detail
                .as_ref()
                .unwrap()
                .cues
                .iter()
                .find(|c| c.number == 5)
                .unwrap()
                .clone();
            assert_eq!(saved.time_ms, 32000);
            assert_eq!(app.position, 32.18);
            assert!(app.preview.request.playing);
            assert_eq!(app.preview.request.revision, before.revision);
            app.position = 70.0;
            click(&mut app, &ctx, size, "Cue 5");
            assert_eq!(app.preview.request.seconds, 32.0);
            assert!(app.preview.request.playing);
            assert_eq!(app.detail.as_ref().unwrap().cues.len(), 5);
            app.inspect(app.tracks[1].clone());
            app.inspect(app.tracks[0].clone());
            assert!(app.detail.as_ref().unwrap().cues.contains(&saved));
        }
    }
}

#[test]
fn holds_edit_the_exact_slot_without_jumping_or_saving_on_release() {
    for touch in [false, true] {
        for number in [2, 8] {
            let size = vec2(800.0, 480.0);
            let (mut app, ctx) = setup();
            app.position = 60.18;
            app.preview.toggle(app.position);
            let original = app.detail.as_ref().unwrap().cues.clone();
            let pos = pad(&mut app, &ctx, size, number).center();
            frame(&mut app, &ctx, size, vec![InputEvent::PointerMoved(pos)]);
            frame(&mut app, &ctx, size, input(pos, TouchPhase::Start, touch));
            advance(&mut app, &ctx, size, 0.3, vec![]);
            assert!(app.cue_editor.is_none());
            advance(&mut app, &ctx, size, 0.31, vec![]);
            assert_eq!(app.cue_editor.as_ref().unwrap().draft.number, number);
            assert_eq!(
                app.cue_editor.as_ref().unwrap().draft.time_ms,
                if number == 2 { 32000 } else { 60000 }
            );
            assert_eq!(app.position, 60.18);
            assert!(!app.preview.request.playing);
            // Pass egui's own touch-context-menu threshold too, then release.
            advance(&mut app, &ctx, size, 0.4, vec![]);
            frame(&mut app, &ctx, size, input(pos, TouchPhase::End, touch));
            assert_eq!(app.cue_editor.as_ref().unwrap().draft.number, number);
            assert_eq!(app.detail.as_ref().unwrap().cues, original);
            assert_eq!(app.position, 60.18);
            click(&mut app, &ctx, size, "Cue cancel");
            assert!(app.cue_editor.is_none());
        }
    }
}

#[test]
fn dragging_away_and_back_cancels_taps_and_holds() {
    for touch in [false, true] {
        for seconds in [0.05, 0.7] {
            let size = vec2(800.0, 480.0);
            let (mut app, ctx) = setup();
            let pos = pad(&mut app, &ctx, size, 7).center();
            frame(&mut app, &ctx, size, vec![InputEvent::PointerMoved(pos)]);
            frame(&mut app, &ctx, size, input(pos, TouchPhase::Start, touch));
            frame(
                &mut app,
                &ctx,
                size,
                input(pos + vec2(20.0, 0.0), TouchPhase::Move, touch),
            );
            advance(
                &mut app,
                &ctx,
                size,
                seconds,
                input(pos, TouchPhase::Move, touch),
            );
            frame(&mut app, &ctx, size, input(pos, TouchPhase::End, touch));
            assert!(app.cue_editor.is_none());
            assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
            click(&mut app, &ctx, size, "Cue 7");
            assert_eq!(app.detail.as_ref().unwrap().cues.len(), 5);
        }
    }
}

#[test]
fn cancelled_touch_and_movement_batched_in_one_frame_never_activate_a_pad() {
    let size = vec2(800.0, 480.0);
    for cancel in [false, true] {
        let (mut app, ctx) = setup();
        let pos = pad(&mut app, &ctx, size, 8).center();
        frame(&mut app, &ctx, size, vec![InputEvent::PointerMoved(pos)]);
        frame(&mut app, &ctx, size, input(pos, TouchPhase::Start, true));
        let events = if cancel {
            let mut events = input(pos, TouchPhase::Cancel, true);
            events.extend(input(pos, TouchPhase::End, true));
            events
        } else {
            vec![
                InputEvent::PointerMoved(pos + vec2(20.0, 0.0)),
                InputEvent::PointerMoved(pos),
            ]
        };
        advance(&mut app, &ctx, size, 0.7, events);
        frame(&mut app, &ctx, size, input(pos, TouchPhase::End, true));
        assert!(app.cue_editor.is_none());
        assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
    }
}

#[test]
fn quick_creation_requires_cues_session_and_grid_but_saved_cues_still_seek() {
    let size = vec2(800.0, 480.0);
    for missing in ["cues", "session", "grid"] {
        let (mut app, ctx) = setup();
        match missing {
            "cues" => app.detail.as_mut().unwrap().cues_ready = false,
            "session" => app.options.demo = false,
            "grid" => app.detail.as_mut().unwrap().beat_grid = Default::default(),
            _ => unreachable!(),
        }
        click(&mut app, &ctx, size, "Cue 6");
        assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
        assert!(app.cue_editor.is_none() && app.cue_quick.is_none());
        click(&mut app, &ctx, size, "Cue 2");
        assert_eq!(app.preview.request.seconds, 32.0);
    }
}

#[test]
fn quick_save_waits_for_verification_and_surfaces_failure_with_the_original_draft() {
    let size = vec2(800.0, 480.0);
    let (mut app, ctx) = setup();
    let (worker, commands, replies) = Worker::test();
    app.worker = Some(worker);
    app.options.demo = false;
    app.library_session = Some("test".into());
    app.position = 32.18;
    click(&mut app, &ctx, size, "Cue 5");
    let Command::Cues {
        track,
        token,
        expected,
        change: Some(Change::Put(cue)),
    } = commands.try_recv().unwrap()
    else {
        panic!("Expected cue write")
    };
    assert!(expected.is_none());
    assert_eq!(cue.time_ms, 32000);
    assert!(app.cue_quick.as_ref().unwrap().pending);
    assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
    click(&mut app, &ctx, size, "Cue 5");
    click(&mut app, &ctx, size, "Cue 6");
    click(&mut app, &ctx, size, "Edit cues");
    assert!(commands.try_recv().is_err());
    // A stale response must not clear this pending write.
    replies
        .send(Event::Cues(
            track.id,
            token.wrapping_sub(1),
            true,
            Err("stale".into()),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(app.cue_quick.is_some());
    replies
        .send(Event::Cues(
            track.id,
            token,
            true,
            Err("Saved, but readback failed; reload before retrying".into()),
        ))
        .unwrap();
    app.position = 80.0;
    app.poll(&ctx);
    assert!(app.cue_quick.is_none());
    let editor = app.cue_editor.as_ref().unwrap();
    assert_eq!(editor.draft, cue);
    assert!(editor.expected.is_none() && !editor.pending);
    assert!(editor.error.contains("readback failed"));
    assert_eq!(app.detail.as_ref().unwrap().cues.len(), 4);
    assert!(commands.try_recv().is_err()); // Never retry an uncertain write automatically.
    click(&mut app, &ctx, size, "Cue reload");
    assert!(matches!(
        commands.try_recv().unwrap(),
        Command::Cues { change: None, .. }
    ));
    let mut cues = app.detail.as_ref().unwrap().cues.clone();
    cues.push(cue.clone());
    replies
        .send(Event::Cues(
            track.id,
            token,
            false,
            Ok(cues::Saved {
                cues,
                persisted: true,
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert_eq!(
        app.cue_editor.as_ref().unwrap().expected.as_ref(),
        Some(&cue)
    );
    click(&mut app, &ctx, size, "Cue cancel");
    click(&mut app, &ctx, size, "Cue 5");
    assert_eq!(app.preview.request.seconds, 32.0);
    assert!(commands.try_recv().is_err());
}

#[test]
fn verified_quick_save_updates_pads_and_late_track_replies_are_ignored() {
    let size = vec2(800.0, 480.0);
    let (mut app, ctx) = setup();
    let (worker, commands, replies) = Worker::test();
    app.worker = Some(worker);
    app.options.demo = false;
    app.library_session = Some("test".into());
    click(&mut app, &ctx, size, "Cue 8");
    let Command::Cues {
        track,
        token,
        change: Some(Change::Put(cue)),
        ..
    } = commands.try_recv().unwrap()
    else {
        panic!("Expected cue write")
    };
    let mut cues = app.detail.as_ref().unwrap().cues.clone();
    cues.push(cue);
    replies
        .send(Event::Cues(
            track.id,
            token,
            true,
            Ok(cues::Saved {
                cues,
                persisted: true,
            }),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(app.cue_quick.is_none() && app.cue_editor.is_none());
    assert_eq!(app.detail.as_ref().unwrap().cues.len(), 5);
    assert_eq!(app.notice, "Cue saved to disk");
    click(&mut app, &ctx, size, "Cue 7");
    assert!(commands.try_recv().is_ok());
    app.inspect(app.tracks[1].clone());
    replies
        .send(Event::Cues(
            track.id,
            token,
            true,
            Err("late failure".into()),
        ))
        .unwrap();
    app.poll(&ctx);
    assert!(app.cue_quick.is_none() && app.cue_editor.is_none());
    assert!(app.detail.is_none());
}
