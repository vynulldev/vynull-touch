// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::Track;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum Sort {
    Title,
    Artist,
    Album,
    Bpm,
    Key,
    Duration,
    Rating,
    Added,
    #[default]
    #[serde(other)]
    Original,
}

impl Sort {
    pub const ALL: [Self; 9] = [
        Self::Original,
        Self::Title,
        Self::Artist,
        Self::Album,
        Self::Bpm,
        Self::Key,
        Self::Duration,
        Self::Rating,
        Self::Added,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Original => "Original",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Bpm => "BPM",
            Self::Key => "Key",
            Self::Duration => "Duration",
            Self::Rating => "Rating",
            Self::Added => "Date added",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(default)]
pub struct Order {
    pub field: Sort,
    pub descending: bool,
}

impl Order {
    pub fn label(self) -> String {
        if self.field == Sort::Original {
            return "Sort: Original".into();
        }
        format!(
            "Sort: {}",
            if self.field == Sort::Added {
                "Added"
            } else {
                self.field.label()
            }
        )
    }

    pub fn apply(self, tracks: &[Track], visible: &mut [usize]) {
        // Stable sorting retains source order for equal values; tracks themselves
        // never move, so selection and player loading remain tied to track IDs.
        if self.field == Sort::Original {
            return;
        }
        match self.field {
            Sort::Title | Sort::Artist | Sort::Album | Sort::Key | Sort::Added => {
                // Normalize once per track instead of allocating in the comparator.
                let keys: Vec<_> = tracks
                    .iter()
                    .map(|t| {
                        let value = match self.field {
                            Sort::Title => &t.title,
                            Sort::Artist => &t.artist,
                            Sort::Album => &t.album,
                            Sort::Key => &t.key,
                            _ => &t.date_added,
                        };
                        normalized(value)
                    })
                    .collect();
                visible.sort_by(|&a, &b| {
                    let order = if self.field == Sort::Key {
                        compare_keys(&keys[a], &keys[b])
                    } else {
                        keys[a].cmp(&keys[b])
                    };
                    self.compare(keys[a].is_empty(), keys[b].is_empty(), order)
                });
            }
            _ => visible.sort_by(|&a, &b| {
                let value = |t: &Track| match self.field {
                    Sort::Bpm => t.bpm,
                    Sort::Duration => f64::from(t.duration),
                    Sort::Rating => f64::from(t.rating),
                    _ => 0.0,
                };
                let (a, b) = (value(&tracks[a]), value(&tracks[b]));
                let missing = |v: f64| !v.is_finite() || (v <= 0.0 && self.field != Sort::Rating);
                self.compare(missing(a), missing(b), a.total_cmp(&b))
            }),
        }
    }

    fn compare(self, missing_a: bool, missing_b: bool, order: Ordering) -> Ordering {
        match (missing_a, missing_b) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            _ => {
                if self.descending {
                    order.reverse()
                } else {
                    order
                }
            }
        }
    }
}

pub fn normalized(text: &str) -> String {
    text.trim().to_lowercase()
}

pub fn compare_keys(a: &str, b: &str) -> Ordering {
    let camelot = |key: &str| {
        let suffix = key.chars().last()?;
        if !matches!(suffix, 'a' | 'b') {
            return None;
        }
        let number = key.get(..key.len() - 1)?.parse::<u8>().ok()?;
        (1..=12).contains(&number).then_some((number, suffix))
    };
    match (camelot(a), camelot(b)) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.cmp(b),
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filters {
    pub min_bpm: Option<f64>,
    pub max_bpm: Option<f64>,
    pub keys: Vec<String>,
    pub genres: Vec<String>,
    pub rating: Option<u8>, // 0 = unrated; 1–5 = minimum stars.
    pub colors: Vec<u8>,
    pub tags: Vec<String>,
}

impl Filters {
    pub fn matches(&self, track: &Track) -> bool {
        if (self.min_bpm.is_some() || self.max_bpm.is_some())
            && (!track.bpm.is_finite()
                || track.bpm <= 0.0
                || self.min_bpm.is_some_and(|min| track.bpm < min)
                || self.max_bpm.is_some_and(|max| track.bpm > max))
        {
            return false;
        }
        let selected = |choices: &[String], value: &str| {
            choices.is_empty() || choices.contains(&normalized(value))
        };
        selected(&self.keys, &track.key)
            && selected(&self.genres, &track.genre)
            && self.rating.is_none_or(|rating| {
                if rating == 0 {
                    track.rating == 0
                } else {
                    track.rating >= rating
                }
            })
            && (self.colors.is_empty() || self.colors.contains(&track.color_id))
            && (self.tags.is_empty()
                || track
                    .tags
                    .iter()
                    .any(|tag| self.tags.contains(&normalized(tag)))
                || (self.tags.iter().any(String::is_empty)
                    && track.tags.iter().all(|tag| tag.trim().is_empty())))
    }

    pub fn count(&self) -> usize {
        [
            self.min_bpm.is_some() || self.max_bpm.is_some(),
            !self.keys.is_empty(),
            !self.genres.is_empty(),
            self.rating.is_some(),
            !self.colors.is_empty(),
            !self.tags.is_empty(),
        ]
        .into_iter()
        .filter(|active| *active)
        .count()
    }

    pub fn summary(&self) -> String {
        let mut labels = Vec::new();
        if self.min_bpm.is_some() || self.max_bpm.is_some() {
            labels.push(match (self.min_bpm, self.max_bpm) {
                (Some(min), Some(max)) => format!("BPM {min}–{max}"),
                (Some(min), _) => format!("BPM ≥ {min}"),
                (_, Some(max)) => format!("BPM ≤ {max}"),
                _ => unreachable!(),
            });
        }
        for (name, values) in [
            ("Key", &self.keys),
            ("Genre", &self.genres),
            ("Tag", &self.tags),
        ] {
            if !values.is_empty() {
                let value = if values.len() == 1 {
                    if values[0].is_empty() {
                        "unset".into()
                    } else {
                        values[0].clone()
                    }
                } else {
                    format!("{} selected", values.len())
                };
                labels.push(format!("{name}: {value}"));
            }
        }
        if let Some(rating) = self.rating {
            labels.push(if rating == 0 {
                "Unrated".into()
            } else {
                format!("Rating {rating}+")
            });
        }
        if !self.colors.is_empty() {
            labels.push(if self.colors.len() == 1 {
                format!("Color: {}", COLORS[usize::from(self.colors[0])])
            } else {
                format!("{} colors", self.colors.len())
            });
        }
        labels.join(" · ")
    }
}

pub const COLORS: [&str; 9] = [
    "No color", "Pink", "Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple",
];

pub fn bpm_range(min: &str, max: &str) -> Result<(Option<f64>, Option<f64>), &'static str> {
    let parse = |text: &str| {
        if text.trim().is_empty() {
            return Ok(None);
        }
        text.trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0 && *value <= 999.0)
            .map(Some)
            .ok_or("Enter BPM greater than 0 and at most 999, or leave it blank.")
    };
    let (min, max) = (parse(min)?, parse(max)?);
    if min.zip(max).is_some_and(|(min, max)| min > max) {
        return Err("Minimum BPM must not exceed maximum BPM.");
    }
    Ok((min, max))
}

#[cfg(test)]
mod tests;
