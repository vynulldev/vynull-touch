// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    editing::same_track,
    grid_edit::{Change as GridChange, Settings, Snapshot},
};
use serde_json::json;

#[cfg(test)]
mod tests;

impl Api {
    fn grid_snapshot(&self, expected: &Track) -> Result<Snapshot, String> {
        let track: Track = self.get(&format!("api/tracks/{}", expected.id))?;
        same_track(expected, &track)?;
        let settings: Settings = self.get(&format!("api/tracks/{}/beats", track.id))?;
        #[derive(Deserialize)]
        struct Analysis {
            #[serde(flatten)]
            grid: TrackAnalysis,
            bpm: f64,
            duration_sec: u32,
        }
        let analysis: Analysis = self.get(&format!("api/analysis/{}", track.id))?;
        if !settings.bpm.is_finite()
            || settings.bpm < 0.0
            || settings.beat_phase_shift > 3
            || !analysis.bpm.is_finite()
            || analysis.bpm < 0.0
        {
            return Err("Invalid grid settings from daemon".into());
        }
        let grid = BeatGrid::new(analysis.grid, track.id)?;
        Ok(Snapshot {
            track,
            grid,
            settings,
            bpm: analysis.bpm,
            duration_sec: analysis.duration_sec,
        })
    }

    fn grid_write(
        &self,
        method: Method,
        path: &str,
        body: serde_json::Value,
    ) -> Result<(), String> {
        #[derive(Deserialize)]
        struct Receipt {
            ok: bool,
        }
        let receipt: Receipt = self.json(method, path, Some(body), 15)?;
        if !receipt.ok {
            return Err("Daemon did not accept the grid edit".into());
        }
        Ok(())
    }

    pub(super) fn edit_grid(
        &self,
        track: &Track,
        expected: Option<&Snapshot>,
        change: Option<GridChange>,
    ) -> Result<Box<Snapshot>, String> {
        self.grid_inner(track, expected, change).map(Box::new).map_err(|error| {
            if change.is_some() {
                format!("Grid edit not confirmed; changes may be partly applied. Reload grid before continuing. {error}")
            } else { error }
        })
    }

    fn grid_inner(
        &self,
        track: &Track,
        expected: Option<&Snapshot>,
        change: Option<GridChange>,
    ) -> Result<Snapshot, String> {
        if change.is_some() && self.session.is_none() {
            return Err(
                "Reconnect to a server with library identity support before editing".into(),
            );
        }
        let before = self.grid_snapshot(track)?;
        let Some(change) = change else {
            return Ok(before);
        };
        if expected.is_none_or(|expected| !before.matches(expected)) {
            return Err("Grid or analysis lock changed elsewhere. Reload before editing.".into());
        }
        let desired = change.apply(&before)?;
        let adjust = "api/analysis/beatgrid/adjust";
        let settings_path = format!("api/tracks/{}/beats", track.id);
        // No retries: relative nudges may already have applied after a lost reply.
        // The daemon has no atomic grid transaction or conditional-write token.
        match change {
            GridChange::Nudge(ms) => self.grid_write(
                Method::POST,
                adjust,
                json!({"track_id":track.id,"offset_ms":ms}),
            )?,
            GridChange::Tempo(bpm) => {
                self.grid_write(Method::POST, adjust, json!({"track_id":track.id,"bpm":bpm}))?;
                self.grid_write(Method::PUT, &settings_path, json!({"bpm":bpm}))?;
            }
            GridChange::Downbeat(_) => {
                let (offset, rotation) = change.alignment(&before.grid)?;
                if offset.abs() > 0.000_001 {
                    // Legacy set_downbeat_ms rebuilds a uniform grid. Instead
                    // shift timestamps, verify, then rotate only the bar phase.
                    self.grid_write(
                        Method::POST,
                        adjust,
                        json!({"track_id":track.id,"offset_ms":offset}),
                    )?;
                    let shifted = self.grid_snapshot(track)?;
                    let mut expected_shift = before.clone();
                    let mut beats = before.grid.beats().to_vec();
                    for beat in &mut beats {
                        beat.time_ms += offset;
                    }
                    expected_shift.grid = BeatGrid::new(
                        TrackAnalysis {
                            track_id: track.id,
                            beats,
                        },
                        track.id,
                    )?;
                    if !shifted.matches(&expected_shift) {
                        return Err("Grid changed during downbeat alignment".into());
                    }
                }
                if rotation != 0 {
                    self.grid_write(
                        Method::PUT,
                        &settings_path,
                        json!({"beat_phase_shift":desired.settings.beat_phase_shift}),
                    )?;
                }
            }
        }
        let after = self.grid_snapshot(track)?;
        if !after.matches(&desired) {
            return Err("Grid readback differs from the requested edit".into());
        }
        Ok(after)
    }
}
