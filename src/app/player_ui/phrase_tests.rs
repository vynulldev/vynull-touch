// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    app::tests::frame,
    phrases::{Phrase, Sections},
    waveform::Style,
};

fn draw(app: &mut App, ctx: &Context, size: Vec2) -> FullOutput {
    frame(app, ctx, size, vec![]);
    let mut output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
            ..Default::default()
        },
        |ui| app.render(ui),
    );
    output.textures_delta.clear();
    output
}

fn labels(output: &FullOutput) -> Vec<&str> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            Shape::Text(text) => Some(text.galley.text()),
            _ => None,
        })
        .collect()
}

#[test]
fn next_cue_countdowns_follow_player_seeks_and_hide_for_unknown_or_stale_sources() {
    for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
        for mode in 0..3 {
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
            app.deck_compare = mode == 1;
            app.deck_focus = (mode == 2).then_some(1);
            app.status.players.truncate(1);
            app.status.players[0].is_playing = false;
            app.status.players[0].play_state_name = "PAUSED".into();
            app.status.players[0].pitch_pct = 8.0;
            app.status.players[0].beat_in_track = 9; // Position 4s.
            let mut assets = Assets::demo(&app.all_tracks[0]);
            assets.cues = vec![
                Cue {
                    number: 2,
                    time_ms: 6000,
                    ..Default::default()
                },
                Cue {
                    number: 3,
                    time_ms: 8000,
                    ..Default::default()
                },
            ];
            app.demo_decks.insert(1, Arc::new(assets));
            for (beat, expected) in [
                (9, "Next B · 1 bar"),
                (13, "Next C · 1 bar"),
                (9, "Next B · 1 bar"),
            ] {
                app.status.players[0].beat_in_track = beat;
                let output = draw(&mut app, &ctx, size);
                assert!(
                    labels(&output)
                        .iter()
                        .any(|label| label.starts_with(expected)),
                    "missing {expected} in mode {mode} at {size:?}"
                );
            }
            // Even a distant cue's full bars/beats readout must fit the small
            // cards without cutting off the beat count.
            let assets = Arc::make_mut(app.demo_decks.get_mut(&1).unwrap());
            assets.cues[0].time_ms = 241_500;
            assets.cues[1].time_ms = 242_000;
            let output = draw(&mut app, &ctx, size);
            let countdown = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    Shape::Text(text)
                        if text.galley.text().starts_with("Next B · 118 bars 3 beats") =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("missing distant cue countdown");
            assert!(
                !countdown.galley.elided,
                "cue countdown clipped in mode {mode} at {size:?}"
            );
            let assets = Arc::make_mut(app.demo_decks.get_mut(&1).unwrap());
            assets.cues[0].time_ms = 6000;
            assets.cues[1].time_ms = 8000;
            // Cue information does not depend on optional phrase analysis.
            Arc::make_mut(app.demo_decks.get_mut(&1).unwrap()).phrases = Default::default();
            assert!(
                labels(&draw(&mut app, &ctx, size))
                    .iter()
                    .any(|label| label.starts_with("Next B · 1 bar"))
            );
            for missing in 0..4 {
                app.status.players[0].beat_in_track = if missing == 0 { 0 } else { 9 };
                app.linked = missing != 1;
                app.status.players[0].external = missing == 2;
                if missing == 3 {
                    app.status.peers.clear();
                }
                let output = draw(&mut app, &ctx, size);
                assert!(
                    !labels(&output)
                        .iter()
                        .any(|label| label.starts_with("Next ")),
                    "stale cue countdown in mode {mode}"
                );
            }
        }
    }
}

#[test]
fn player_phrases_align_in_every_view_and_hide_live_countdowns_without_valid_timing() {
    for size in [vec2(800.0, 480.0), vec2(1280.0, 800.0)] {
        for mode in 0..3 {
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
            app.deck_compare = mode == 1;
            app.deck_focus = (mode == 2).then_some(1);
            app.deck_span = 16.0;
            app.status.players[0].is_playing = false;
            app.status.players[0].play_state_name = "PAUSED".into();
            app.status.players[0].beat_in_track = 9; // Saved beat at 4s, independent of pitch.
            app.status.players[0].pitch_pct = 8.0;
            let mut assets = Assets::demo(&app.all_tracks[0]);
            assets.phrases = Arc::new(
                Sections::new(
                    [(1, 0, 6000), (5, 6000, 10000), (6, 12000, 16000)]
                        .into_iter()
                        .map(|(kind, start_ms, end_ms)| Phrase {
                            kind,
                            name: String::new(),
                            start_ms,
                            end_ms,
                        })
                        .collect(),
                )
                .unwrap(),
            );
            app.demo_decks.insert(1, Arc::new(assets));
            for style in [Style::Rgb, Style::Blue, Style::ThreeBand] {
                app.waveform_style = style;
                let output = draw(&mut app, &ctx, size);
                assert!(labels(&output).contains(&"INTRO · CHORUS in 1 bar"));
                let waves: &[(&str, bool)] = match mode {
                    0 => &[("Deck waveform 1", false)],
                    1 => &[("Compare waveform 1", true), ("Compare overview 1", false)],
                    _ => &[
                        ("Player overview waveform", false),
                        ("Player detail waveform", true),
                    ],
                };
                for &(name, detailed) in waves {
                    let rect = app.hits.iter().find(|(label, _)| label == name).unwrap().1;
                    let strip = rect.with_min_y(rect.bottom() - crate::phrases::STRIP_HEIGHT);
                    let chorus = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            Shape::Rect(r)
                                if r.fill == Color32::from_rgb(255, 106, 0)
                                    && strip.contains_rect(r.rect) =>
                            {
                                Some(r.rect)
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| panic!("missing phrase strip in {name} at {size:?}"));
                    let range = if detailed {
                        focus::detail_range(4.0, if mode == 1 { 16.0 * 1.08 } else { 16.0 })
                    } else {
                        0.0..284.0
                    };
                    let expected = strip.left()
                        + ((6.0 - range.start) / (range.end - range.start)) as f32 * strip.width();
                    assert!(
                        (chorus.left() - expected).abs() < 0.01,
                        "phrase boundary drift in {name}"
                    );
                    for shape in &output.shapes {
                        if let Shape::Text(text) = &shape.shape
                            && text.galley.text().contains("CHORUS in")
                            && detailed
                        {
                            assert!(text.pos.y + text.galley.size().y < strip.top());
                        }
                    }
                }
            }
            // Static waveforms retain their phrases while missing beat counters
            // and stale/disconnected players must not claim a current section.
            app.status.players[0].beat_in_track = 0;
            let output = draw(&mut app, &ctx, size);
            assert!(labels(&output).contains(&"Phrase position unavailable"));
            assert!(
                !labels(&output)
                    .iter()
                    .any(|label| label.contains("CHORUS in"))
            );
            app.status.players[0].beat_in_track = 9;
            app.linked = false;
            let output = draw(&mut app, &ctx, size);
            assert!(labels(&output).contains(&"Phrase position unavailable"));
            app.linked = true;
            app.status.players[0].external = true; // Colliding local ID cannot supply phrases.
            let output = draw(&mut app, &ctx, size);
            assert!(
                !labels(&output)
                    .iter()
                    .any(|label| label.contains("CHORUS") || label.contains("Phrase position"))
            );
            assert!(app.deck_snapshot(1).assets.phrases.is_empty());
        }
    }
}
