// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::Cue;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Region {
    pub start: f64,
    pub end: f64,
}

impl Region {
    pub fn validate(self, duration: f64) -> Result<Self, String> {
        if !self.start.is_finite()
            || !self.end.is_finite()
            || !duration.is_finite()
            || self.start < 0.0
            || self.end <= self.start
            || self.end > duration
        {
            return Err("Loop In and Out must be inside the track, with Out after In".into());
        }
        Ok(self)
    }
    pub fn contains(self, seconds: f64) -> bool {
        seconds >= self.start && seconds < self.end
    }
    pub fn position(self, seconds: f64) -> f64 {
        if self.contains(seconds) {
            seconds
        } else {
            self.start
        }
    }
    pub fn from_cue(cue: &Cue, duration: f64) -> Result<Self, String> {
        crate::cues::validate(cue, duration)?;
        Self {
            start: f64::from(cue.time_ms) / 1000.0,
            end: f64::from(cue.loop_ms) / 1000.0,
        }
        .validate(duration)
    }
    pub fn apply_to(self, cue: &mut Cue) -> Result<(), String> {
        self.validate(f64::from(i32::MAX) / 1000.0)?;
        let start = (self.start * 1000.0).round() as u32;
        let end = (self.end * 1000.0).round() as i32;
        if i64::from(end) <= i64::from(start) {
            return Err("Loop is shorter than one millisecond".into());
        }
        cue.kind = "loop".into();
        cue.time_ms = start;
        cue.loop_ms = end;
        Ok(())
    }
}

/// Half-open sample-frame boundaries shared by the producer and playback clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frames {
    pub start: u64,
    pub end: u64,
}
impl Frames {
    pub fn new(region: Region, rate: u32, frames: u64) -> Result<Self, String> {
        region.validate(frames as f64 / f64::from(rate))?;
        let range = Self {
            start: (region.start * f64::from(rate)).round() as u64,
            end: (region.end * f64::from(rate)).round().min(frames as f64) as u64,
        };
        if range.start >= range.end {
            return Err("Loop is shorter than one audio frame".into());
        }
        Ok(range)
    }
    pub fn position(self, start: u64, consumed: u64) -> u64 {
        self.start + (start - self.start + consumed) % (self.end - self.start)
    }
    pub fn constrain(self, frame: u64) -> u64 {
        if (self.start..self.end).contains(&frame) {
            frame
        } else {
            self.start
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sample_boundaries_preserve_fractional_grid_times_and_reject_invalid_ranges() {
        let region = Region {
            start: 0.40025,
            end: 2.42075,
        };
        assert_eq!(
            Frames::new(region, 48000, 144000).unwrap(),
            Frames {
                start: 19212,
                end: 116196
            }
        );
        for region in [
            Region {
                start: -1.0,
                end: 1.0,
            },
            Region {
                start: 1.0,
                end: 1.0,
            },
            Region {
                start: 0.0,
                end: 4.0,
            },
            Region {
                start: f64::NAN,
                end: 1.0,
            },
            Region {
                start: 1.0,
                end: 1.000001,
            },
        ] {
            assert!(Frames::new(region, 48000, 144000).is_err());
        }
        let bounds = Frames { start: 10, end: 14 };
        assert_eq!(bounds.constrain(14), 10);
        assert_eq!(bounds.position(12, 1001), 13);
    }
}
