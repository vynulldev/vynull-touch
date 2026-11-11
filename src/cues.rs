// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{beat_grid::BeatGrid, model::Cue};
use eframe::egui::Color32;

#[derive(Clone, Debug)]
pub enum Change {
    Put(Cue),
    Delete(u16),
}
impl Change {
    pub fn number(&self) -> u16 {
        match self {
            Self::Put(c) => c.number,
            Self::Delete(n) => *n,
        }
    }
    pub fn desired(&self) -> Option<&Cue> {
        match self {
            Self::Put(c) => Some(c),
            Self::Delete(_) => None,
        }
    }
}
pub struct Saved {
    pub cues: Vec<Cue>,
    pub persisted: bool,
}

pub struct Editor {
    pub expected: Option<Cue>,
    pub draft: Cue,
    pub snap: bool,
    pub typing: bool,
    pub pending: bool,
    pub error: String,
    pub loop_template: Option<crate::loops::Region>,
    default_colors: [u32; 8],
}
impl Editor {
    pub fn new(
        cues: &[Cue],
        slot: u16,
        position: f64,
        duration: f64,
        grid: &BeatGrid,
        default_colors: [u32; 8],
    ) -> Self {
        let expected = cues.iter().find(|c| c.number == slot).cloned();
        let draft = expected.clone().unwrap_or(Cue {
            number: slot,
            kind: "cue".into(),
            loop_ms: -1,
            color_id: slot
                .checked_sub(1)
                .and_then(|index| default_colors.get(usize::from(index)))
                .copied()
                .unwrap_or(0),
            ..Default::default()
        });
        let mut editor = Self {
            expected,
            draft,
            snap: !grid.is_empty(),
            typing: false,
            pending: false,
            error: String::new(),
            loop_template: None,
            default_colors,
        };
        if editor.expected.is_none() {
            editor.place(position, duration, grid);
        }
        editor
    }
    pub fn select(
        &self,
        cues: &[Cue],
        slot: u16,
        position: f64,
        duration: f64,
        grid: &BeatGrid,
    ) -> Self {
        let mut editor = Self::new(cues, slot, position, duration, grid, self.default_colors);
        editor.loop_template = self.loop_template;
        if let Some(region) = self.loop_template {
            if let Err(error) = region
                .apply_to(&mut editor.draft)
                .and_then(|()| validate(&editor.draft, duration))
            {
                editor.error = error;
            } else {
                editor.error.clear();
            }
        }
        editor
    }
    pub fn place(&mut self, seconds: f64, duration: f64, grid: &BeatGrid) {
        let result = placement(seconds, duration, self.snap.then_some(grid));
        match result {
            Ok(time_ms) => {
                let mut candidate = self.draft.clone();
                if candidate.kind == "loop" {
                    let end = i64::from(candidate.loop_ms) + i64::from(time_ms)
                        - i64::from(candidate.time_ms);
                    if end > i64::from(i32::MAX) || end < 0 {
                        self.error = "Loop end is outside this track".into();
                        return;
                    }
                    candidate.loop_ms = end as i32;
                }
                candidate.time_ms = time_ms;
                match validate(&candidate, duration) {
                    Ok(()) => {
                        self.draft = candidate;
                        if self.loop_template.is_some() {
                            self.loop_template =
                                crate::loops::Region::from_cue(&self.draft, duration).ok();
                        }
                        self.error.clear();
                    }
                    Err(e) => self.error = e,
                }
            }
            Err(e) => self.error = e,
        }
    }
}

pub fn placement(seconds: f64, duration: f64, grid: Option<&BeatGrid>) -> Result<u32, String> {
    if !seconds.is_finite()
        || !duration.is_finite()
        || duration <= 0.0
        || seconds < 0.0
        || seconds > duration
        || duration * 1000.0 > u32::MAX as f64
    {
        return Err("Cue position must be inside the track".into());
    }
    let seconds = match grid {
        Some(g) => g
            .nearest(seconds, duration)
            .ok_or("No beat inside the track; turn Snap off")?,
        None => seconds,
    };
    Ok((seconds * 1000.0).round().min((duration * 1000.0).floor()) as u32)
}
pub fn validate(cue: &Cue, duration: f64) -> Result<(), String> {
    placement(f64::from(cue.time_ms) / 1000.0, duration, None)?;
    if cue.number == 0 {
        return Err("Choose a cue slot".into());
    }
    if cue.label.chars().count() > 128 {
        return Err("Cue labels may contain at most 128 characters".into());
    }
    match cue.kind.as_str() {
        "cue" if cue.loop_ms == -1 => Ok(()),
        "loop"
            if i64::from(cue.loop_ms) > i64::from(cue.time_ms)
                && f64::from(cue.loop_ms) <= duration * 1000.0 =>
        {
            Ok(())
        }
        _ => Err("Loop endpoints must be inside the track, with Out after In".into()),
    }
}
pub fn slot(number: u16) -> String {
    if (1..=8).contains(&number) {
        char::from(b'A' + (number - 1) as u8).to_string()
    } else {
        format!("M{}", number.saturating_sub(8))
    }
}
pub fn timestamp(ms: u32) -> String {
    format!("{}:{:02}.{:03}", ms / 60000, ms / 1000 % 60, ms % 1000)
}

// The 16 swatches sampled from rekordbox-cue-color-selection.png, in reading
// order. These are exact matches in the 62-color cue palette below; retain the
// native IDs so saved cues and deck colors agree. Default (0) is separate.
pub const PALETTE: [(u32, &str); 16] = [
    (0x31, "Magenta"),    // #DE44CF
    (0x38, "Purple"),     // #B432FF
    (0x3c, "Lavender"),   // #AA72FF
    (0x3e, "Periwinkle"), // #6473FF
    (0x01, "Blue"),
    (0x05, "Sky blue"),
    (0x09, "Cyan"),
    (0x0e, "Teal"),
    (0x12, "Emerald"),
    (0x16, "Green"),
    (0x1a, "Lime"),
    (0x1e, "Yellow green"),
    (0x20, "Gold"),
    (0x26, "Orange"),
    (0x2a, "Red"),
    (0x2d, "Pink"),
];
pub fn color(id: u32) -> Color32 {
    const HEX: [u32; 63] = [
        0x28e214, 0x305aff, 0x5073ff, 0x508cff, 0x50a0ff, 0x50b4ff, 0x50b0f2, 0x50aee8, 0x45acdb,
        0x00e0ff, 0x19daf0, 0x32d2e6, 0x21b4b9, 0x20aaa0, 0x1fa392, 0x19a08c, 0x14a584, 0x14aa7d,
        0x10b176, 0x30d26e, 0x37de5a, 0x3ceb50, 0x28e214, 0x7dc13d, 0x8cc832, 0x9bd723, 0xa5e116,
        0xa5dc0a, 0xaad208, 0xb4c805, 0xb4be04, 0xbab404, 0xc3af04, 0xe1aa00, 0xffa000, 0xff9600,
        0xff8c00, 0xff7500, 0xe0641b, 0xe0461e, 0xe0301e, 0xe02823, 0xe62828, 0xff376f, 0xff2d6f,
        0xff127b, 0xf51e8c, 0xeb2da0, 0xe637b4, 0xde44cf, 0xde448d, 0xe630b4, 0xe619dc, 0xe600ff,
        0xdc00ff, 0xcc00ff, 0xb432ff, 0xb93cff, 0xc542ff, 0xaa5aff, 0xaa72ff, 0x8272ff, 0x6473ff,
    ];
    let id = if id > 0x3e {
        id.saturating_sub(0x30)
    } else {
        id
    };
    let hex = HEX.get(id as usize).copied().unwrap_or(HEX[0]);
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beat_grid::{Beat, TrackAnalysis};
    #[test]
    fn defaults_apply_to_empty_hot_slots_and_never_replace_existing_colors() {
        let grid = BeatGrid::default();
        let colors = [49, 56, 60, 62, 1, 5, 9, 14];
        let cues = [Cue {
            number: 2,
            kind: "cue".into(),
            loop_ms: -1,
            color_id: 99,
            ..Default::default()
        }];
        let mut edit = Editor::new(&cues, 1, 1.0, 10.0, &grid, colors);
        assert_eq!(edit.draft.color_id, 49);
        edit.draft.color_id = 42;
        edit.place(2.0, 10.0, &grid);
        assert_eq!(edit.draft.color_id, 42);
        edit = edit.select(&cues, 2, 1.0, 10.0, &grid);
        assert_eq!(edit.draft, cues[0]);
        edit = edit.select(&cues, 8, 1.0, 10.0, &grid);
        assert_eq!(edit.draft.color_id, 14);
        edit = edit.select(&cues, 9, 1.0, 10.0, &grid);
        assert_eq!(edit.draft.color_id, 0);
    }

    #[test]
    fn snap_uses_saved_grid_and_free_placement_keeps_milliseconds() {
        let grid = BeatGrid::new(
            TrackAnalysis {
                track_id: 1,
                beats: [-150.0, 350.25, 850.5, 1350.75]
                    .into_iter()
                    .enumerate()
                    .map(|(i, time_ms)| Beat {
                        time_ms,
                        beat_in_bar: (i + 1) as u8,
                    })
                    .collect(),
            },
            1,
        )
        .unwrap();
        assert_eq!(placement(0.79, 1.2, Some(&grid)).unwrap(), 851);
        assert_eq!(placement(0.0, 1.2, Some(&grid)).unwrap(), 350);
        assert_eq!(placement(0.791, 1.2, None).unwrap(), 791);
        assert!(placement(0.1, 0.2, Some(&grid)).is_err());
        assert!(placement(f64::NAN, 1.0, None).is_err());
    }
    #[test]
    fn moving_loop_preserves_length_label_and_unknown_color_and_cancel_is_local() {
        let cue = Cue {
            number: 9,
            kind: "loop".into(),
            time_ms: 1000,
            loop_ms: 3000,
            color_id: 99,
            label: "Break".into(),
        };
        let mut edit = Editor::new(
            std::slice::from_ref(&cue),
            9,
            4.0,
            10.0,
            &BeatGrid::default(),
            [0; 8],
        );
        edit.place(4.0, 10.0, &BeatGrid::default());
        assert_eq!((edit.draft.time_ms, edit.draft.loop_ms), (4000, 6000));
        assert_eq!(edit.draft.color_id, 99);
        assert_eq!(edit.expected, Some(cue));
        edit.place(9.0, 10.0, &BeatGrid::default());
        assert!(!edit.error.is_empty());
        assert_eq!(edit.draft.loop_ms, 6000);
        assert_eq!(slot(9), "M1");
        assert_eq!(color(0x42), color(0x12));
    }
    #[test]
    fn moving_new_loop_then_selecting_slot_preserves_edited_endpoints() {
        let grid = BeatGrid::default();
        let mut edit = Editor::new(&[], 1, 1.0, 10.0, &grid, [0; 8]);
        edit.loop_template = Some(crate::loops::Region {
            start: 1.0,
            end: 3.0,
        });
        edit = edit.select(&[], 1, 1.0, 10.0, &grid);
        edit.place(4.0, 10.0, &grid);
        edit = edit.select(&[], 8, 1.0, 10.0, &grid);
        assert_eq!((edit.draft.time_ms, edit.draft.loop_ms), (4000, 6000));
        assert_eq!(edit.draft.number, 8);
        assert!(edit.expected.is_none());
        assert_eq!(edit.draft.kind, "loop");
    }
}
