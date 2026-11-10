// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

pub(super) fn bars_and_beats(beats: f64) -> String {
    // Count up to the next beat, matching the phrase countdown. The epsilon
    // only removes floating-point noise; cue timestamps are never snapped.
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
}

pub(super) fn next_cue(assets: &Assets, position: f64) -> Option<(&Cue, f64)> {
    if !position.is_finite() || position < 0.0 {
        return None;
    }
    assets
        .cues
        .iter()
        .filter(|cue| {
            let seconds = f64::from(cue.time_ms) / 1000.0;
            seconds > position && (assets.duration <= 0.0 || seconds <= assets.duration)
        })
        .min_by_key(|cue| (cue.time_ms, cue.number))
        .map(|cue| (cue, f64::from(cue.time_ms) / 1000.0 - position))
}

pub(super) fn cue_summary(
    assets: &Assets,
    position: Option<f64>,
    detailed: bool,
) -> Option<String> {
    let position = position?;
    let (cue, seconds) = next_cue(assets, position)?;
    let target = f64::from(cue.time_ms) / 1000.0;
    let beats = if assets.grid.is_empty() {
        // External analysis supplies saved beat times without bar phase. Beat
        // distance still works, without assigning downbeats or using local IDs.
        crate::beat_grid::beats_between(&assets.beats, position, target, |s| *s)
    } else {
        assets.grid.beats_between(position, target)
    };
    let distance = beats.map(bars_and_beats);
    let mut text = format!(
        "Next {} · {}",
        crate::cues::slot(cue.number),
        distance
            .as_deref()
            .map_or_else(|| format!("+{}", time(seconds)), str::to_owned)
    );
    if detailed {
        if distance.is_some() {
            text.push_str(&format!(" · +{}", time(seconds)));
        }
        if !cue.label.is_empty() {
            text.push_str(&format!(" · {}", cue.label));
        }
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat_grid::{Beat, BeatGrid, TrackAnalysis};

    fn assets() -> Assets {
        let beats = vec![0.0, 0.5, 1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        Assets {
            duration: 10.0,
            bpm: 120.0,
            grid: Arc::new(
                BeatGrid::new(
                    TrackAnalysis {
                        track_id: 1,
                        beats: beats
                            .iter()
                            .enumerate()
                            .map(|(i, seconds)| Beat {
                                time_ms: seconds * 1000.0,
                                beat_in_bar: (i % 4 + 1) as u8,
                            })
                            .collect(),
                    },
                    1,
                )
                .unwrap(),
            ),
            beats,
            // Intentionally unsorted, including a loop start, off-grid memory
            // cue and a stale point past the end of the track.
            cues: vec![
                Cue {
                    number: 3,
                    time_ms: 7000,
                    kind: "loop".into(),
                    loop_ms: 8000,
                    ..Default::default()
                },
                Cue {
                    number: 9,
                    time_ms: 6500,
                    ..Default::default()
                },
                Cue {
                    number: 2,
                    time_ms: 6000,
                    label: "Drop".into(),
                    ..Default::default()
                },
                Cue {
                    number: 1,
                    time_ms: 0,
                    ..Default::default()
                },
                Cue {
                    number: 4,
                    time_ms: 11000,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn next_cue_counts_saved_intervals_through_tempo_changes_seeks_and_loop_starts() {
        let assets = assets();
        for (position, expected) in [
            (0.0, "Next B · 2 bars"),
            (1.5, "Next B · 1 bar 1 beat"),
            (2.0, "Next B · 1 bar"),
            (2.5, "Next B · 1 bar"),
            (3.0, "Next B · 3 beats"),
            (5.9999, "Next B · 1 beat"),
            (6.0, "Next M1 · 1 beat"),
            (6.5, "Next C · 1 beat"),
            (2.0, "Next B · 1 bar"),
        ] {
            assert_eq!(
                cue_summary(&assets, Some(position), false).as_deref(),
                Some(expected)
            );
        }
        assert_eq!(
            cue_summary(&assets, Some(2.0), true).as_deref(),
            Some("Next B · 1 bar · +0:04 · Drop")
        );
        for position in [
            None,
            Some(-1.0),
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(7.0),
            Some(10.0),
        ] {
            assert!(cue_summary(&assets, position, true).is_none());
        }
    }

    #[test]
    fn next_cue_uses_external_beat_times_and_falls_back_to_seconds_without_grid_coverage() {
        let mut assets = assets();
        assets.grid = Default::default();
        // External analysis has no downbeat phase; its own timestamps suffice.
        assert_eq!(
            cue_summary(&assets, Some(2.0), false).as_deref(),
            Some("Next B · 1 bar")
        );
        assets.beats.clear();
        assert_eq!(
            cue_summary(&assets, Some(2.0), false).as_deref(),
            Some("Next B · +0:04")
        );
        assert_eq!(
            cue_summary(&assets, Some(2.0), true).as_deref(),
            Some("Next B · +0:04 · Drop")
        );
        assets = self::assets();
        assets.cues = vec![Cue {
            number: 8,
            time_ms: 9000,
            ..Default::default()
        }];
        // Do not extrapolate past the last saved beat, even with known BPM.
        assert_eq!(
            cue_summary(&assets, Some(2.0), false).as_deref(),
            Some("Next H · +0:07")
        );
        assets.cues.clear();
        assert!(cue_summary(&assets, Some(2.0), false).is_none());
    }
}
