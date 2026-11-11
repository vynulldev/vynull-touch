// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    beat_grid::{Beat, BeatGrid, TrackAnalysis},
    model::Track,
};
use serde::Deserialize;

mod snapshot;
pub use snapshot::RawGrid;
pub mod history;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Settings {
    pub bpm: f64,
    pub beat_phase_shift: u8,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub raw: Option<RawGrid>,
    pub track: Track,
    pub grid: BeatGrid,
    pub bpm: f64,
    pub duration_sec: u32,
    pub settings: Settings,
}

impl Snapshot {
    pub fn matches(&self, other: &Self) -> bool {
        self.track.analysis_locked == other.track.analysis_locked && self.matches_grid(other)
    }

    pub fn matches_grid(&self, other: &Self) -> bool {
        self.raw == other.raw
            && self.track.id == other.track.id
            && self.track.file_path == other.track.file_path
            && (self.track.bpm - other.track.bpm).abs() < 0.000_001
            && self.settings == other.settings
            && (self.bpm - other.bpm).abs() < 0.000_001
            && self.duration_sec == other.duration_sec
            && self.grid.matches(&other.grid)
    }
    pub fn restore(&self, target: &Self) -> Result<Self, &'static str> {
        if self.track.analysis_locked {
            return Err("Turn off Analysis lock before Undo or Redo");
        }
        if self.track.id != target.track.id
            || self.track.file_path != target.track.file_path
            || self.duration_sec != target.duration_sec
        {
            return Err("Track source or duration changed; reload the grid");
        }
        if self.raw.is_none() || target.raw.is_none() {
            return Err("Update Vynull to enable grid Undo/Redo");
        }
        let mut restored = target.clone();
        restored.track = self.track.clone();
        restored.track.bpm = target.track.bpm;
        Ok(restored)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Change {
    Lock(bool),
    Nudge(f64),
    Downbeat(f64),
    Tempo(f64),
}

impl Change {
    pub fn alignment(self, grid: &BeatGrid) -> Result<(f64, u8), &'static str> {
        let Self::Downbeat(ms) = self else {
            return Err("Not a downbeat edit");
        };
        if !ms.is_finite() {
            return Err("Invalid playhead position");
        }
        let nearest = grid
            .beats()
            .iter()
            .min_by(|a, b| (a.time_ms - ms).abs().total_cmp(&(b.time_ms - ms).abs()))
            .ok_or("No grid to align")?;
        Ok((ms - nearest.time_ms, nearest.beat_in_bar - 1))
    }

    pub fn apply(self, before: &Snapshot) -> Result<Snapshot, &'static str> {
        let lock_change = matches!(self, Self::Lock(_));
        if before.track.analysis_locked && !lock_change {
            return Err("Analysis is locked. Turn off Analysis lock before editing.");
        }
        if before.grid.is_empty() && !lock_change {
            return Err("No grid to edit; analyze the track first");
        }
        let mut after = before.clone();
        let mut beats = before.grid.beats().to_vec();
        match self {
            Self::Lock(locked) => {
                after.track.analysis_locked = locked;
                return Ok(after);
            }
            Self::Nudge(ms) => {
                if !ms.is_finite() || ms.abs() > 1000.0 {
                    return Err("Nudge must be within one second");
                }
                for beat in &mut beats {
                    beat.time_ms += ms;
                }
            }
            Self::Downbeat(ms) => {
                let (offset, rotation) = self.alignment(&before.grid)?;
                if ms < 0.0 || ms >= f64::from(before.duration_sec) * 1000.0 {
                    return Err("Place the playhead inside the track");
                }
                for beat in &mut beats {
                    beat.time_ms += offset;
                    beat.beat_in_bar = (beat.beat_in_bar - 1 + 4 - rotation) % 4 + 1;
                }
                after.settings.beat_phase_shift = (before.settings.beat_phase_shift + rotation) % 4;
            }
            Self::Tempo(bpm) => {
                if !bpm.is_finite() || !(20.0..=400.0).contains(&bpm) {
                    return Err("BPM must be between 20 and 400");
                }
                let first = beats[0];
                let interval = 60000.0 / bpm;
                let duration = f64::from(before.duration_sec) * 1000.0;
                let count = ((duration - first.time_ms) / interval).ceil();
                if !(1.0..=1_000_000.0).contains(&count) {
                    return Err("Track duration or grid origin is invalid");
                }
                beats.clear();
                let mut time_ms = first.time_ms;
                while time_ms < duration {
                    if beats.len() >= 1_000_000 {
                        return Err("Grid exceeds the beat limit");
                    }
                    beats.push(Beat {
                        time_ms,
                        beat_in_bar: ((usize::from(first.beat_in_bar) - 1 + beats.len()) % 4 + 1)
                            as u8,
                    });
                    time_ms += interval;
                }
                after.bpm = bpm;
                after.settings.bpm = bpm;
                after.track.bpm = bpm;
            }
        }
        if let Some(raw) = &mut after.raw {
            raw.apply(self, before);
        }
        after.grid = BeatGrid::new(
            TrackAnalysis {
                track_id: before.track.id,
                beats,
            },
            before.track.id,
        )?;
        Ok(after)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            raw: None,
            track: Track {
                id: 1,
                ..Default::default()
            },
            bpm: 120.0,
            duration_sec: 3,
            settings: Settings {
                bpm: 120.0,
                beat_phase_shift: 1,
            },
            grid: BeatGrid::new(
                TrackAnalysis {
                    track_id: 1,
                    beats: [-100.0, 400.0, 925.0, 1460.0, 2000.0]
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
            .unwrap(),
        }
    }
    #[test]
    fn alignment_preserves_variable_intervals_negative_beats_and_bar_rotation() {
        let before = snapshot();
        let after = Change::Downbeat(950.0).apply(&before).unwrap();
        assert_eq!(
            after.grid.beats()[2],
            Beat {
                time_ms: 950.0,
                beat_in_bar: 1
            }
        );
        assert_eq!(
            after.grid.beats()[0],
            Beat {
                time_ms: -75.0,
                beat_in_bar: 3
            }
        );
        assert_eq!(after.settings.beat_phase_shift, 3);
        for (a, b) in before
            .grid
            .beats()
            .windows(2)
            .zip(after.grid.beats().windows(2))
        {
            assert_eq!(a[1].time_ms - a[0].time_ms, b[1].time_ms - b[0].time_ms);
        }
        assert_eq!(
            Change::Downbeat(150.0).alignment(&before.grid).unwrap(),
            (250.0, 0)
        );
        let nudged = Change::Nudge(-0.1).apply(&before).unwrap();
        assert_eq!(nudged.grid.beats()[0].time_ms, -100.1);
        assert_eq!(nudged.grid.beats()[2].beat_in_bar, 3);
        for change in [
            Change::Nudge(f64::INFINITY),
            Change::Downbeat(f64::NAN),
            Change::Downbeat(3000.0),
        ] {
            assert!(change.apply(&before).is_err());
        }
    }
    #[test]
    fn tempo_correction_keeps_grid_origin_and_rejects_invalid_or_locked_edits() {
        let before = snapshot();
        let after = Change::Tempo(60.0).apply(&before).unwrap();
        assert_eq!(
            after
                .grid
                .beats()
                .iter()
                .map(|b| b.time_ms)
                .collect::<Vec<_>>(),
            vec![-100.0, 900.0, 1900.0, 2900.0]
        );
        assert_eq!(after.bpm, 60.0);
        assert_eq!(after.track.bpm, 60.0);
        assert_eq!(after.settings.bpm, 60.0);
        for bpm in [0.0, 401.0, f64::NAN] {
            assert!(Change::Tempo(bpm).apply(&before).is_err());
        }
        let mut locked = before.clone();
        locked.track.analysis_locked = true;
        assert!(Change::Nudge(1.0).apply(&locked).is_err());
        let mut empty = before;
        empty.grid = Default::default();
        assert!(Change::Tempo(120.0).apply(&empty).is_err());
    }
    #[test]
    fn analysis_lock_preserves_grid_and_metadata_and_can_unlock_without_beats() {
        for empty in [false, true] {
            for locked in [false, true] {
                let mut before = snapshot();
                before.track.analysis_locked = locked;
                if empty {
                    before.grid = Default::default();
                }
                let after = Change::Lock(!locked).apply(&before).unwrap();
                assert_eq!(after.track.analysis_locked, !locked);
                let mut restored = after;
                restored.track.analysis_locked = locked;
                assert!(restored.matches(&before));
            }
        }
    }
    #[test]
    fn raw_snapshots_preserve_underlying_overrides_and_reject_inconsistent_responses() {
        let mut before = snapshot();
        before.track.file_path = "/music/track.flac".into();
        before.track.bpm = 120.0;
        before.raw = Some(RawGrid::from_snapshot(&before));
        assert!(before.raw.as_ref().unwrap().consistent(&before));
        for change in [
            Change::Nudge(-0.1),
            Change::Downbeat(950.0),
            Change::Tempo(60.0),
        ] {
            let after = change.apply(&before).unwrap();
            assert!(after.raw.as_ref().unwrap().consistent(&after));
            let mut renamed = after.clone();
            renamed.track.title = "New title".into();
            let restored = renamed.restore(&before).unwrap();
            assert!(restored.matches(&before));
            assert_eq!(restored.track.title, "New title");
        }
        let mut overridden = before.clone();
        let constant = Change::Tempo(100.0).apply(&before).unwrap();
        overridden.grid = constant.grid;
        overridden.bpm = 100.0;
        overridden.track.bpm = 100.0;
        overridden.settings.bpm = 100.0;
        overridden.raw.as_mut().unwrap().library_bpm = 100.0;
        overridden.raw.as_mut().unwrap().detected_bpm = 120.0;
        assert!(overridden.raw.as_ref().unwrap().consistent(&overridden));
        let edited = Change::Tempo(90.0).apply(&overridden).unwrap();
        let restored = edited.restore(&overridden).unwrap();
        assert!(restored.matches(&overridden));
        assert_eq!(
            restored.raw.as_ref().unwrap().beats_ms,
            before.raw.as_ref().unwrap().beats_ms
        );
        let mut invalid = restored.raw.clone().unwrap();
        invalid.beats_ms[1] = invalid.beats_ms[0];
        assert!(!invalid.consistent(&restored));
        invalid = restored.raw.clone().unwrap();
        invalid.file_path = "/other.flac".into();
        assert!(!invalid.consistent(&restored));
        invalid = restored.raw.clone().unwrap();
        invalid.version = 2;
        assert!(!invalid.consistent(&restored));
    }

    #[test]
    fn grid_history_bounds_saved_states_and_discards_redo_on_new_edit() {
        use history::{Direction, History};
        let mut history = History::default();
        let mut state = snapshot();
        for _ in 0..100 {
            history.record(state.clone());
            state = Change::Nudge(1.0).apply(&state).unwrap();
        }
        for _ in 0..64 {
            let restored = history.target(Direction::Undo).unwrap().clone();
            history.restored(Direction::Undo, state);
            state = restored;
        }
        assert!(history.target(Direction::Undo).is_none());
        assert_eq!(state.grid.beats()[0].time_ms, -64.0);
        assert!(history.target(Direction::Redo).is_some());
        history.record(state);
        assert!(history.target(Direction::Redo).is_none());
    }
}
