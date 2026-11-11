// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde::Serialize;

/// Raw analysis plus library overrides, supplied by Vynull for exact restoration.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct RawGrid {
    pub version: u8,
    pub file_path: String,
    pub bpm: f64,
    pub duration_sec: u32,
    pub downbeat_index: usize,
    #[serde(deserialize_with = "crate::model::null_vec")]
    pub beats_ms: Vec<f64>,
    pub library_bpm: f64,
    pub detected_bpm: f64,
    pub beat_phase_shift: u8,
}

impl RawGrid {
    pub fn from_snapshot(s: &Snapshot) -> Self {
        Self {
            version: 1,
            file_path: s.track.file_path.clone(),
            bpm: s.bpm,
            duration_sec: s.duration_sec,
            downbeat_index: s.grid.beats().first().map_or(0, |b| {
                usize::from((5 - b.beat_in_bar + 4 - s.settings.beat_phase_shift) % 4)
            }),
            beats_ms: s.grid.beats().iter().map(|b| b.time_ms).collect(),
            library_bpm: s.track.bpm,
            detected_bpm: 0.0,
            beat_phase_shift: s.settings.beat_phase_shift,
        }
    }

    pub fn consistent(&self, snapshot: &Snapshot) -> bool {
        if self.version != 1
            || self.file_path != snapshot.track.file_path
            || self.duration_sec != snapshot.duration_sec
            || self.downbeat_index > 1_000_000
            || self.beats_ms.len() > 1_000_000
            || self.beat_phase_shift > 3
            || self.beat_phase_shift != snapshot.settings.beat_phase_shift
            || self.library_bpm != snapshot.track.bpm
            || self.library_bpm != snapshot.settings.bpm
            || [self.bpm, self.library_bpm, self.detected_bpm]
                .iter()
                .any(|b| !b.is_finite() || !(0.0..=1000.0).contains(b))
            || self.beats_ms.iter().any(|b| !b.is_finite())
            || self.beats_ms.windows(2).any(|b| b[0] >= b[1])
        {
            return false;
        }
        let overridden =
            self.library_bpm > 0.0 && self.detected_bpm > 0.0 && self.library_bpm != self.bpm;
        let bpm = if overridden {
            self.library_bpm
        } else {
            self.bpm
        };
        if bpm != snapshot.bpm {
            return false;
        }
        let mut times = self.beats_ms.clone();
        if overridden {
            times.clear();
            let mut time = self.beats_ms.first().copied().unwrap_or(0.0);
            while time < f64::from(self.duration_sec) * 1000.0 {
                if times.len() >= 1_000_000 {
                    return false;
                }
                times.push(time);
                time += 60000.0 / bpm;
            }
        }
        let phase = (self.downbeat_index + usize::from(self.beat_phase_shift)) % 4;
        times.len() == snapshot.grid.beats().len()
            && times
                .iter()
                .zip(snapshot.grid.beats())
                .enumerate()
                .all(|(i, (time, beat))| {
                    (time - beat.time_ms).abs() < 0.01
                        && beat.beat_in_bar == ((i + 4 - phase) % 4 + 1) as u8
                })
    }

    pub(super) fn apply(&mut self, change: Change, before: &Snapshot) {
        match change {
            Change::Lock(_) => (),
            Change::Nudge(ms) => self.beats_ms.iter_mut().for_each(|b| *b += ms),
            Change::Downbeat(_) => {
                let (offset, rotation) = change.alignment(&before.grid).unwrap();
                self.beats_ms.iter_mut().for_each(|b| *b += offset);
                self.beat_phase_shift = (self.beat_phase_shift + rotation) % 4;
            }
            Change::Tempo(bpm) => {
                let mut time = self.beats_ms.first().copied().unwrap_or(0.0);
                self.beats_ms.clear();
                while time < f64::from(self.duration_sec) * 1000.0 {
                    self.beats_ms.push(time);
                    time += 60000.0 / bpm;
                }
                self.bpm = bpm;
                if self.detected_bpm == 0.0 {
                    self.detected_bpm = self.library_bpm;
                }
                self.library_bpm = bpm;
            }
        }
    }
}
