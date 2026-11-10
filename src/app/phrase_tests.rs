// SPDX-License-Identifier: GPL-3.0-or-later
use super::tests::{click, frame};
use super::*;

fn rect(app: &App, name: &str) -> Rect {
    app.hits.iter().find(|(label, _)| label == name).unwrap().1
}

#[test]
fn inspect_phrases_align_with_both_waveforms_and_remain_touch_seekable() {
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
        let (preview, requests, _) = Controller::test();
        app.preview = preview;
        let mut track = app.tracks[0].clone();
        track.duration = 240;
        app.inspect(track.clone());
        app.detail.as_mut().unwrap().phrases = crate::phrases::Sections::new(vec![
            crate::phrases::Phrase {
                kind: 1,
                name: "INTRO".into(),
                start_ms: 1000,
                end_ms: 32_000,
            },
            crate::phrases::Phrase {
                kind: 5,
                name: "CHORUS".into(),
                start_ms: 32_000,
                end_ms: 64_000,
            },
            crate::phrases::Phrase {
                kind: 3,
                name: "DOWN".into(),
                start_ms: 64_000,
                end_ms: 96_000,
            },
        ])
        .unwrap();
        for zoom in [1.0, 8.0, 32.0] {
            app.zoom = zoom;
            app.position = 64.0;
            frame(&mut app, &ctx, size, vec![]);
            let mut output = ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                    ..Default::default()
                },
                |ui| app.render(ui),
            );
            output.textures_delta.clear();
            for (name, waveform, range) in [
                ("Full phrases", "Full waveform", 0.0..240.0),
                (
                    "Detail phrases",
                    "Detail waveform",
                    app.waveform_window(&track),
                ),
            ] {
                let strip = rect(&app, name);
                let wave = rect(&app, waveform);
                assert!(wave.contains_rect(strip));
                assert_eq!(strip.x_range(), wave.x_range());
                assert_eq!(strip.bottom(), wave.bottom());
                let chorus = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Rect(r)
                            if r.fill == Color32::from_rgb(255, 106, 0)
                                && strip.contains_rect(r.rect) =>
                        {
                            Some(r.rect)
                        }
                        _ => None,
                    })
                    .unwrap();
                let boundary = strip.left()
                    + ((64.0 - range.start) / (range.end - range.start)) as f32 * strip.width();
                assert!(
                    (chorus.right() - boundary).abs() < 0.01,
                    "saved boundary drifted at {zoom}x"
                );
                assert!(strip.bottom() < rect(&app, "Cue 1").top());
            }
            for name in ["Cue 8", "Inspect load", "Preview play", "Loop controls"] {
                assert!(
                    rect(&app, name).bottom() <= size.y - 26.0,
                    "{name} must stay visible"
                );
            }
        }
        // The strip shares the waveform's touch gesture and time mapping.
        // A tap seeks within the section, rather than jumping to its start.
        app.inspect_quantize = false;
        app.preview.toggle(app.position);
        frame(&mut app, &ctx, size, vec![]);
        let strip = rect(&app, "Full phrases");
        let pos = pos2(strip.left() + strip.width() * 0.25, strip.center().y);
        for (phase, pressed) in [(TouchPhase::Start, true), (TouchPhase::End, false)] {
            frame(
                &mut app,
                &ctx,
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
        assert!((app.position - 60.0).abs() < 0.001);
        assert!(requests.borrow().playing);
        click(&mut app, &ctx, size, "Grid");
        assert!(app.hits.iter().any(|(name, _)| name == "Full phrases"));
        app.detail.as_mut().unwrap().phrases = Default::default();
        frame(&mut app, &ctx, size, vec![]);
        assert!(
            !app.hits
                .iter()
                .any(|(name, _)| name == "Full phrases" || name == "Detail phrases")
        );
        assert_eq!(rect(&app, "Full waveform").height(), 64.0);
    }
}
