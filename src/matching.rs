// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::{Player, Track};

#[derive(Clone, Debug)]
pub struct Match {
    pub id: u32,
    pub path: String,
    pub reason: String,
    distance: f64,
    key_rank: u8,
}

pub struct State {
    pub seed: Option<Player>,
    pub tolerance: f64,
    pub keys: bool,
    pub half_double: bool,
    pub selected: Option<u32>,
    pub results: Vec<Match>,
    pub dirty: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            seed: None,
            tolerance: 6.0,
            keys: true,
            half_double: false,
            selected: None,
            results: vec![],
            dirty: true,
        }
    }
}

pub fn tempo(player: &Player) -> Option<f64> {
    let bpm = player.bpm * (1.0 + player.pitch_pct / 100.0);
    (bpm.is_finite() && bpm > 0.0).then_some(bpm)
}

impl State {
    pub fn select_player(&mut self, player: Player) {
        self.seed = Some(player);
        self.dirty = true;
        self.selected = None;
    }

    pub fn update(&mut self, tracks: &[Track]) {
        if !self.dirty {
            return;
        }
        self.dirty = false;
        self.results.clear();
        let Some(seed) = &self.seed else {
            return;
        };
        let Some(target) = tempo(seed) else {
            self.selected = None;
            return;
        };
        for track in tracks {
            if track.file_missing
                || !track.bpm.is_finite()
                || track.bpm <= 0.0
                || (!seed.external && seed.source.is_empty() && seed.track_id == track.id)
            {
                continue;
            }
            let relation = relation(&seed.key, &track.key);
            if self.keys && relation.is_none() {
                continue;
            }
            let factors: &[f64] = if self.half_double {
                &[1.0, 0.5, 2.0]
            } else {
                &[1.0]
            };
            let (factor, delta) = factors
                .iter()
                .map(|factor| (*factor, (target / (track.bpm * factor) - 1.0) * 100.0))
                .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                .unwrap();
            if delta.abs() > self.tolerance {
                continue;
            }
            let key_reason = relation.map_or("Key unrestricted", |(_, reason)| reason);
            self.results.push(Match {
                id: track.id,
                path: track.file_path.clone(),
                reason: format!(
                    "{key_reason} · {delta:+.1}% tempo{}",
                    if factor == 0.5 {
                        " · half time"
                    } else if factor == 2.0 {
                        " · double time"
                    } else {
                        ""
                    }
                ),
                distance: delta.abs(),
                key_rank: relation.map_or(2, |(rank, _)| rank),
            });
        }
        self.results.sort_by(|a, b| {
            a.key_rank
                .cmp(&b.key_rank)
                .then_with(|| a.distance.total_cmp(&b.distance))
                .then_with(|| a.id.cmp(&b.id))
        });
        if self
            .selected
            .is_some_and(|id| !self.results.iter().any(|m| m.id == id))
        {
            self.selected = None;
        }
    }
}

// Same wheel positions as Vynull's analysis/key.go, including enharmonic names.
fn key(text: &str) -> Option<(u8, bool)> {
    let normalized = text
        .trim()
        .replace('♭', "b")
        .replace('♯', "#")
        .to_uppercase();
    if let Some(suffix) = normalized.chars().last()
        && matches!(suffix, 'A' | 'B')
        && let Ok(n) = normalized[..normalized.len() - 1].parse::<u8>()
        && (1..=12).contains(&n)
    {
        return Some((n, suffix == 'A'));
    }
    let minor = normalized.ends_with('M');
    let note = if minor {
        &normalized[..normalized.len() - 1]
    } else {
        &normalized
    };
    let pc = match note {
        "C" | "B#" => 0,
        "C#" | "DB" => 1,
        "D" => 2,
        "D#" | "EB" => 3,
        "E" | "FB" => 4,
        "F" | "E#" => 5,
        "F#" | "GB" => 6,
        "G" => 7,
        "G#" | "AB" => 8,
        "A" => 9,
        "A#" | "BB" => 10,
        "B" | "CB" => 11,
        _ => return None,
    };
    Some((
        if minor {
            [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10][pc]
        } else {
            [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1][pc]
        },
        minor,
    ))
}

fn relation(a: &str, b: &str) -> Option<(u8, &'static str)> {
    let (a, b) = (key(a)?, key(b)?);
    if a == b {
        Some((0, "Same key"))
    } else if a.0 == b.0 {
        Some((1, "Relative key"))
    } else if a.1 == b.1 && matches!(a.0.abs_diff(b.0), 1 | 11) {
        Some((1, "Adjacent key"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
