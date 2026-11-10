// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::null_vec;
use serde::Deserialize;

/// The daemon applies saved tempo and bar-phase edits to this response.
#[derive(Deserialize)]
pub struct TrackAnalysis {
    pub track_id: u32,
    #[serde(default, deserialize_with = "null_vec")]
    pub beats: Vec<Beat>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
pub struct Beat {
    pub time_ms: f64,
    pub beat_in_bar: u8,
}

impl Beat {
    pub fn seconds(&self) -> f64 {
        self.time_ms / 1000.0
    }
}

#[derive(Clone, Debug, Default)]
pub struct BeatGrid {
    beats: Vec<Beat>,
    downbeats: Vec<Beat>,
}

impl BeatGrid {
    pub fn beats(&self) -> &[Beat] {
        &self.beats
    }

    pub fn matches(&self, other: &Self) -> bool {
        self.beats.len() == other.beats.len()
            && self.beats.iter().zip(&other.beats).all(|(a, b)| {
                (a.time_ms - b.time_ms).abs() < 0.01 && a.beat_in_bar == b.beat_in_bar
            })
    }

    pub fn new(analysis: TrackAnalysis, expected: u32) -> Result<Self, &'static str> {
        if analysis.track_id != expected {
            return Err("Invalid beat grid track identity");
        }
        if analysis
            .beats
            .iter()
            .any(|b| !b.time_ms.is_finite() || !(1..=4).contains(&b.beat_in_bar))
            || analysis
                .beats
                .windows(2)
                .any(|pair| pair[0].time_ms >= pair[1].time_ms)
        {
            return Err("Invalid beat grid timing or bar positions");
        }
        // Preserve negative leading beats and the supplied bar phase. Clip only
        // when drawing; regenerating from track BPM would lose grid corrections.
        let downbeats = analysis
            .beats
            .iter()
            .copied()
            .filter(|b| b.beat_in_bar == 1)
            .collect();
        Ok(Self {
            beats: analysis.beats,
            downbeats,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.beats.is_empty()
    }

    /// Distance in saved beat coordinates, including fractional beats and tempo
    /// changes. Unlike seeking, a countdown must not extrapolate beyond the grid.
    pub fn beats_between(&self, start: f64, end: f64) -> Option<f64> {
        beats_between(&self.beats, start, end, |beat| beat.seconds())
    }

    /// One-based saved beat index and authoritative position within its bar.
    /// The cursor readout allows 1 ms at a boundary: persisted cues round to
    /// milliseconds, and the preview clock floors seeks to an audio frame.
    /// This tolerance affects the label only, never snapping or seek timing.
    /// Negative leading beats count toward the index. Do not invent a beat
    /// outside that tolerance, or continue the last beat beyond one saved interval.
    pub fn beat_at(&self, seconds: f64) -> Option<(usize, u8)> {
        if !seconds.is_finite() || seconds < 0.0 {
            return None;
        }
        let index = self
            .beats
            .partition_point(|b| b.seconds() <= seconds + 0.001)
            .checked_sub(1)?;
        let beat = self.beats[index];
        if index + 1 == self.beats.len() && seconds > beat.seconds() {
            let previous = self.beats.get(index.checked_sub(1)?)?;
            if seconds >= beat.seconds() + (beat.seconds() - previous.seconds()) {
                return None;
            }
        }
        Some((index + 1, beat.beat_in_bar))
    }

    pub fn nearest(&self, seconds: f64, duration: f64) -> Option<f64> {
        if !seconds.is_finite() || !duration.is_finite() || duration < 0.0 {
            return None;
        }
        let first = self.beats.partition_point(|b| b.seconds() < 0.0);
        let last = self.beats.partition_point(|b| b.seconds() <= duration);
        let beats = &self.beats[first..last.max(first)];
        let i = beats.partition_point(|b| b.seconds() < seconds);
        [i.checked_sub(1).and_then(|n| beats.get(n)), beats.get(i)]
            .into_iter()
            .flatten()
            .min_by(|a, b| {
                (a.seconds() - seconds)
                    .abs()
                    .total_cmp(&(b.seconds() - seconds).abs())
            })
            .map(|b| b.seconds())
    }

    /// Move in saved beat coordinates, retaining the fractional position within
    /// a beat even across tempo changes. At the grid edges, use the adjacent
    /// saved interval and clamp to the audio boundaries, never the metadata BPM.
    pub fn jump(&self, seconds: f64, count: i32, duration: f64) -> Option<f64> {
        if self.beats.len() < 2
            || !seconds.is_finite()
            || !duration.is_finite()
            || duration <= 0.0
            || count == 0
            || !(-64..=64).contains(&count)
        {
            return None;
        }
        let seconds = seconds.clamp(0.0, duration);
        let last_interval = self.beats.len() - 2;
        let index = self
            .beats
            .partition_point(|b| b.seconds() <= seconds)
            .saturating_sub(1)
            .min(last_interval);
        let start = self.beats[index].seconds();
        let interval = self.beats[index + 1].seconds() - start;
        let beat = index as f64 + (seconds - start) / interval + f64::from(count);
        let target_index = (beat.floor().max(0.0) as usize).min(last_interval);
        let target_start = self.beats[target_index].seconds();
        let target_interval = self.beats[target_index + 1].seconds() - target_start;
        let target = target_start + (beat - target_index as f64) * target_interval;
        target.is_finite().then(|| target.clamp(0.0, duration))
    }

    pub fn loop_region(
        &self,
        seconds: f64,
        count: usize,
        duration: f64,
    ) -> Result<crate::loops::Region, String> {
        if !(1..=64).contains(&count) {
            return Err("Choose a loop length from 1 to 64 beats".into());
        }
        let start = self
            .nearest(seconds, duration)
            .ok_or("No beat grid available for looping")?;
        let index = self.beats.partition_point(|b| b.seconds() < start);
        let end = self
            .beats
            .get(index + count)
            .ok_or("Not enough beats remain for this loop")?
            .seconds();
        crate::loops::Region { start, end }
            .validate(duration)
            .map_err(|_| "Not enough beats remain for this loop".into())
    }

    pub fn loop_beats(&self, region: crate::loops::Region, duration: f64) -> Option<usize> {
        (1..=64).find(|&n| {
            self.loop_region(region.start, n, duration).is_ok_and(|r| {
                (r.start - region.start).abs() <= 0.001 && (r.end - region.end).abs() <= 0.001
            })
        })
    }

    /// As in the desktop client, use exact timestamps and a bounded line budget:
    /// all beats, then downbeats, then globally aligned powers-of-two bars.
    pub fn visible(
        &self,
        start: f64,
        end: f64,
        max_lines: usize,
    ) -> impl Iterator<Item = &Beat> + Clone {
        let range = |beats: &[Beat]| {
            let first = beats.partition_point(|b| b.seconds() < start.max(0.0));
            let last = beats.partition_point(|b| b.seconds() <= end);
            first..last.max(first)
        };
        let (mut beats, mut stride) = (&[][..], 1);
        if start.is_finite() && end.is_finite() && end >= start && max_lines > 0 {
            let all = range(&self.beats);
            if all.len() <= max_lines {
                beats = &self.beats[all];
            } else {
                let bars = range(&self.downbeats);
                stride = bars.len().div_ceil(max_lines).max(1).next_power_of_two();
                let first = bars.start.div_ceil(stride) * stride;
                beats = &self.downbeats[first.min(bars.end)..bars.end];
            }
        }
        beats.iter().step_by(stride)
    }
}

/// Count intervals in validated, increasing saved beat timestamps. Bar phase is
/// not needed for distance, so external player analysis can use this as well.
pub(crate) fn beats_between<T>(
    beats: &[T],
    start: f64,
    end: f64,
    seconds: impl Fn(&T) -> f64,
) -> Option<f64> {
    if !start.is_finite() || !end.is_finite() || start < 0.0 || end < start {
        return None;
    }
    let coordinate = |position: f64| {
        if beats.len() < 2
            || position < seconds(beats.first()?)
            || position > seconds(beats.last()?)
        {
            return None;
        }
        let index = beats
            .partition_point(|beat| seconds(beat) <= position)
            .saturating_sub(1)
            .min(beats.len() - 2);
        let left = seconds(&beats[index]);
        let right = seconds(&beats[index + 1]);
        Some(index as f64 + (position - left) / (right - left))
    };
    Some(coordinate(end)? - coordinate(start)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phrase_countdowns_count_saved_intervals_across_tempo_changes_without_extrapolation() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 7,
                beats: [
                    0.0, 500.0, 1000.0, 1500.0, 2000.0, 3000.0, 4000.0, 5000.0, 6000.0,
                ]
                .into_iter()
                .enumerate()
                .map(|(i, time_ms)| Beat {
                    time_ms,
                    beat_in_bar: ((i + 2) % 4 + 1) as u8,
                })
                .collect(),
            },
            7,
        )
        .unwrap();
        assert_eq!(grid.beats_between(0.0, 6.0), Some(8.0));
        assert_eq!(grid.beats_between(1.25, 3.5), Some(3.0));
        assert_eq!(grid.beats_between(2.5, 6.0), Some(3.5));
        assert_eq!(grid.beats_between(6.0, 6.0), Some(0.0));
        for (start, end) in [
            (-1.0, 2.0),
            (6.0, 7.0),
            (3.0, 2.0),
            (f64::NAN, 2.0),
            (0.0, f64::INFINITY),
        ] {
            assert_eq!(grid.beats_between(start, end), None);
        }
        assert_eq!(BeatGrid::default().beats_between(0.0, 6.0), None);
    }

    #[test]
    fn cursor_beat_numbers_preserve_leading_beats_and_saved_bar_phase() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: [(-125.0, 3), (400.25, 4), (990.5, 1), (1510.0, 2)]
                    .map(|(time_ms, beat_in_bar)| Beat {
                        time_ms,
                        beat_in_bar,
                    })
                    .to_vec(),
            },
            1,
        )
        .unwrap();
        assert_eq!(grid.beat_at(0.0), Some((1, 3)));
        assert_eq!(grid.beat_at(0.399), Some((1, 3)));
        assert_eq!(grid.beat_at(0.4), Some((2, 4)));
        assert_eq!(grid.beat_at(0.40025), Some((2, 4)));
        assert_eq!(grid.beat_at(0.989), Some((2, 4)));
        assert_eq!(grid.beat_at(0.99), Some((3, 1)));
        assert_eq!(grid.beat_at(0.9905), Some((3, 1)));
        assert_eq!(grid.beat_at(1.6), Some((4, 2)));
        assert_eq!(grid.beat_at(2.1), None);
        assert_eq!(grid.beat_at(-1.0), None);
        assert_eq!(grid.beat_at(f64::NAN), None);
        assert_eq!(BeatGrid::default().beat_at(0.0), None);
        let single = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: vec![Beat {
                    time_ms: 100.0,
                    beat_in_bar: 4,
                }],
            },
            1,
        )
        .unwrap();
        assert_eq!(single.beat_at(0.0), None);
        assert_eq!(single.beat_at(0.0995), Some((1, 4)));
        assert_eq!(single.beat_at(0.1), Some((1, 4)));
        assert_eq!(single.beat_at(0.2), None);
    }

    #[test]
    fn beat_jumps_preserve_phase_through_saved_tempo_changes_and_clamp_at_audio_edges() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: [-100.0, 400.0, 1000.0, 1400.0, 2200.0, 2800.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, time_ms)| Beat {
                        time_ms,
                        beat_in_bar: (i % 4 + 1) as u8,
                    })
                    .collect(),
            },
            1,
        )
        .unwrap();
        assert_eq!(grid.jump(0.4, 2, 3.0), Some(1.4));
        let forward = grid.jump(0.55, 2, 3.0).unwrap();
        assert!((forward - 1.6).abs() < 1e-9);
        assert!((grid.jump(forward, -2, 3.0).unwrap() - 0.55).abs() < 1e-9);
        assert_eq!(grid.jump(0.0, -1, 3.0), Some(0.0));
        assert_eq!(grid.jump(0.55, -32, 3.0), Some(0.0));
        assert_eq!(grid.jump(0.55, 32, 3.0), Some(3.0));
        assert_eq!(grid.jump(3.0, 1, 3.0), Some(3.0));
        assert!((grid.jump(3.0, -1, 3.0).unwrap() - 2.4).abs() < 1e-9);
        for (seconds, count, duration) in [
            (f64::NAN, 1, 3.0),
            (0.5, 1, f64::INFINITY),
            (0.5, 1, 0.0),
            (0.5, 1, -1.0),
            (0.5, 0, 3.0),
            (0.5, i32::MIN, 3.0),
        ] {
            assert_eq!(grid.jump(seconds, count, duration), None);
        }
        assert_eq!(BeatGrid::default().jump(0.5, 1, 3.0), None);
        let single = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: vec![Beat {
                    time_ms: 0.0,
                    beat_in_bar: 1,
                }],
            },
            1,
        )
        .unwrap();
        assert_eq!(single.jump(0.5, 1, 3.0), None);
    }

    #[test]
    fn loops_count_saved_beats_instead_of_extrapolating_bpm() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: [-100.0, 400.25, 910.5, 1405.0, 1910.25, 2420.75, 2918.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, time_ms)| Beat {
                        time_ms,
                        beat_in_bar: (i % 4 + 1) as u8,
                    })
                    .collect(),
            },
            1,
        )
        .unwrap();
        let region = grid.loop_region(0.41, 4, 3.0).unwrap();
        assert_eq!(
            region,
            crate::loops::Region {
                start: 0.40025,
                end: 2.42075
            }
        );
        assert_eq!(grid.loop_beats(region, 3.0), Some(4));
        let mut cue = crate::model::Cue::default();
        region.apply_to(&mut cue).unwrap();
        assert_eq!((cue.time_ms, cue.loop_ms), (400, 2421));
        assert!(grid.loop_region(1.4, 4, 3.0).is_err());
        assert!(grid.loop_region(0.4, 4, 2.4).is_err());
        assert!(grid.loop_region(0.4, 0, 3.0).is_err());
        assert!(BeatGrid::default().loop_region(0.0, 4, 3.0).is_err());
    }

    #[test]
    fn preserves_fractional_timing_and_bar_phase_when_clipping() {
        let beats = [
            (-125.0, 1),
            (360.25, 2),
            (870.0, 3),
            (1380.0, 4),
            (1905.75, 1),
        ]
        .map(|(time_ms, beat_in_bar)| Beat {
            time_ms,
            beat_in_bar,
        });
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 7,
                beats: beats.to_vec(),
            },
            7,
        )
        .unwrap();
        assert_eq!(
            grid.visible(0.0, 2.0, 100).copied().collect::<Vec<_>>(),
            beats[1..]
        );
        assert_eq!(
            grid.visible(0.8, 1.4, 100).copied().collect::<Vec<_>>(),
            beats[2..4]
        );
        assert_eq!(
            grid.visible(0.0, 2.0, 2).copied().collect::<Vec<_>>(),
            [beats[4]]
        );
    }

    #[test]
    fn dense_views_keep_authoritative_downbeats_bounded_and_stable() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: (0..100_000)
                    .map(|i| Beat {
                        time_ms: f64::from(i) * 500.0 + 125.0,
                        beat_in_bar: (i % 4 + 1) as u8,
                    })
                    .collect(),
            },
            1,
        )
        .unwrap();
        for budget in [1, 20, 100, 1000] {
            let beats = grid.visible(0.0, 50_000.0, budget).collect::<Vec<_>>();
            assert!(beats.len() <= budget);
            assert!(beats.iter().all(|b| b.beat_in_bar == 1));
            assert_eq!(beats[0].seconds(), 0.125);
        }
        assert_eq!(
            grid.visible(100.0, 1100.0, 50).collect::<Vec<_>>(),
            grid.visible(100.1, 1100.1, 50).collect::<Vec<_>>()
        );
        assert!(grid.visible(10.0, 5.0, 100).next().is_none());
        assert!(grid.visible(0.0, 100.0, 0).next().is_none());
        assert!(grid.visible(f64::NAN, 100.0, 100).next().is_none());
    }

    #[test]
    fn missing_grids_are_empty_and_malformed_grids_are_rejected() {
        for json in [r#"{"track_id":1,"beats":null}"#, r#"{"track_id":1}"#] {
            let analysis = serde_json::from_str(json).unwrap();
            assert!(BeatGrid::new(analysis, 1).unwrap().is_empty());
        }
        assert!(
            BeatGrid::new(
                TrackAnalysis {
                    track_id: 2,
                    beats: vec![]
                },
                1
            )
            .is_err()
        );
        for beats in [
            vec![Beat {
                time_ms: f64::NAN,
                beat_in_bar: 1,
            }],
            vec![Beat {
                time_ms: 100.0,
                beat_in_bar: 0,
            }],
            vec![Beat {
                time_ms: 100.0,
                beat_in_bar: 5,
            }],
            vec![
                Beat {
                    time_ms: 100.0,
                    beat_in_bar: 1,
                },
                Beat {
                    time_ms: 100.0,
                    beat_in_bar: 2,
                },
            ],
            vec![
                Beat {
                    time_ms: 200.0,
                    beat_in_bar: 1,
                },
                Beat {
                    time_ms: 100.0,
                    beat_in_bar: 2,
                },
            ],
        ] {
            assert!(BeatGrid::new(TrackAnalysis { track_id: 1, beats }, 1).is_err());
        }
    }
}
