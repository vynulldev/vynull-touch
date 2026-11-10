// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::ops::Range;

pub(super) const HEADER_HEIGHT: f32 = 20.0;

/// Reserve a strip within the existing panel, keeping its time mapping and
/// playhead identical to the waveform. Empty analysis takes no space.
pub(super) fn strip(
    p: &Painter,
    rect: Rect,
    assets: &Assets,
    range: Range<f64>,
    position: Option<f64>,
) -> Rect {
    if assets.phrases.is_empty() || range.end <= range.start {
        return rect;
    }
    let strip = rect.with_min_y(rect.bottom() - crate::phrases::STRIP_HEIGHT);
    assets.phrases.paint(p, strip, range.clone());
    if let Some(position) = position.filter(|pos| range.contains(pos)) {
        let x = strip.left()
            + ((position - range.start) / (range.end - range.start)) as f32 * strip.width();
        p.vline(x, strip.y_range(), Stroke::new(2.0, TEXT));
    }
    rect.with_max_y(strip.top() - 2.0)
}

fn summary(assets: &Assets, position: Option<f64>) -> String {
    let Some(position) = position.filter(|pos| pos.is_finite() && *pos >= 0.0) else {
        return "Phrase position unavailable".into();
    };
    let (current, next) = assets.phrases.at(position);
    let Some(next) = next else {
        return current.map_or_else(
            || "No phrase at playhead".into(),
            |phrase| format!("{} · Final section", phrase.label()),
        );
    };
    let distance = assets
        .grid
        .beats_between(position, next.start())
        .map(|beats| {
            // Round up to the upcoming beat, with only a floating-point epsilon.
            // Phrase boundaries themselves retain their exact saved timestamps.
            let beats = ((beats - 1e-7).ceil() as usize).max(1);
            let bars = beats / 4;
            let beats = beats % 4;
            match (bars, beats) {
                (0, beats) => format!("{beats} {}", if beats == 1 { "beat" } else { "beats" }),
                (bars, 0) => format!("{bars} {}", if bars == 1 { "bar" } else { "bars" }),
                (bars, beats) => format!(
                    "{bars} {} {beats} {}",
                    if bars == 1 { "bar" } else { "bars" },
                    if beats == 1 { "beat" } else { "beats" }
                ),
            }
        });
    let upcoming = match distance {
        Some(distance) => format!("{} in {distance}", next.label()),
        None => format!("Next {}", next.label()),
    };
    current.map_or_else(
        || upcoming.clone(),
        |phrase| format!("{} · {upcoming}", phrase.label()),
    )
}

pub(super) fn paint_summary(
    p: &Painter,
    rect: Rect,
    assets: &Assets,
    position: Option<f64>,
    size: f32,
) {
    if !assets.phrases.is_empty() {
        theme::text(
            p,
            rect,
            summary(assets, position),
            size,
            if position.is_some() { TEXT } else { MUTED },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        beat_grid::{Beat, BeatGrid, TrackAnalysis},
        phrases::{Phrase, Sections},
    };

    #[test]
    fn countdowns_follow_exact_phrase_boundaries_seeks_gaps_and_saved_tempo() {
        let mut assets = Assets {
            phrases: Arc::new(
                Sections::new(
                    [
                        (1, 1000, 2000),
                        (2, 2000, 6000),
                        (5, 6000, 8000),
                        (6, 9000, 10000),
                    ]
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
            ),
            grid: Arc::new(
                BeatGrid::new(
                    TrackAnalysis {
                        track_id: 7,
                        beats: [
                            0.0, 500.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0,
                            7000.0, 8000.0, 9000.0, 10000.0,
                        ]
                        .into_iter()
                        .enumerate()
                        .map(|(i, time_ms)| Beat {
                            time_ms,
                            beat_in_bar: (i % 4 + 1) as u8,
                        })
                        .collect(),
                    },
                    7,
                )
                .unwrap(),
            ),
            ..Default::default()
        };
        for (position, expected) in [
            (0.0, "INTRO in 2 beats"),
            (1.0, "INTRO · UP in 2 beats"),
            (2.0, "UP · CHORUS in 1 bar"),
            (2.5, "UP · CHORUS in 1 bar"),
            (3.0, "UP · CHORUS in 3 beats"),
            (5.9999, "UP · CHORUS in 1 beat"),
            (6.0, "CHORUS · OUTRO in 3 beats"),
            (8.0, "OUTRO in 1 beat"),
            (9.0, "OUTRO · Final section"),
            (10.0, "No phrase at playhead"),
            (2.0, "UP · CHORUS in 1 bar"), // Seek/loop back is immediate.
        ] {
            assert_eq!(summary(&assets, Some(position)), expected);
        }
        for position in [None, Some(f64::NAN), Some(-1.0)] {
            assert_eq!(summary(&assets, position), "Phrase position unavailable");
        }
        assets.grid = Default::default();
        assets.bpm = 120.0;
        assert_eq!(summary(&assets, Some(2.0)), "UP · Next CHORUS");
    }
}
