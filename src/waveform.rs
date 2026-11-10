// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::WaveEntry;
use eframe::egui::{Color32, Painter, Rect, pos2};
use serde::{Deserialize, Serialize};

pub mod detail;
pub mod envelope;
pub mod markers;

const BASS: Color32 = Color32::from_rgb(0, 90, 255);
const MID: Color32 = Color32::from_rgb(255, 174, 0);
const HIGH: Color32 = Color32::from_rgb(255, 246, 219);

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
pub enum Style {
    #[serde(rename = "rgb")]
    Rgb,
    #[serde(rename = "3band")]
    ThreeBand,
    #[serde(rename = "blue")]
    Blue,
    #[default]
    #[serde(rename = "auto", other)]
    Auto,
}

impl Style {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Rgb, Self::ThreeBand, Self::Blue];

    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Rgb => "RGB",
            Self::ThreeBand => "3-band",
            Self::Blue => "Blue",
        }
    }

    pub fn uses_bands(self, available: bool) -> bool {
        available && matches!(self, Self::Auto | Self::ThreeBand)
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Overview {
    Half,
    #[default]
    #[serde(other)]
    Full,
}

impl Overview {
    pub fn label(self) -> &'static str {
        match self {
            Self::Half => "Half",
            Self::Full => "Full",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
pub struct BandEntry {
    pub bass: u8,
    pub mid: u8,
    pub high: u8,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Bands {
    pub track_id: u32,
    pub style: String,
    pub sample_rate: u32,
    #[serde(deserialize_with = "crate::model::null_vec")]
    pub entries: Vec<BandEntry>,
}

impl Bands {
    pub fn available(&self) -> bool {
        self.sample_rate > 0 && !self.entries.is_empty()
    }

    pub fn peak(&self, start: f64, end: f64) -> Option<BandEntry> {
        if !self.available() || end <= 0.0 || !start.is_finite() || !end.is_finite() {
            return None;
        }
        let first = (start.max(0.0) * f64::from(self.sample_rate)).floor() as usize;
        let last = (end.max(0.0) * f64::from(self.sample_rate)).ceil() as usize;
        self.entries
            .get(first.min(self.entries.len())..last.max(first).min(self.entries.len()))
            .and_then(band_peak)
    }
}

// Preserve each band's transients independently when reducing to screen pixels.
pub fn band_peak(entries: &[BandEntry]) -> Option<BandEntry> {
    entries.iter().copied().reduce(|a, b| BandEntry {
        bass: a.bass.max(b.bass),
        mid: a.mid.max(b.mid),
        high: a.high.max(b.high),
    })
}

pub fn summarize_bands(entries: &[BandEntry]) -> Vec<BandEntry> {
    entries
        .chunks(entries.len().div_ceil(1024).max(1))
        .filter_map(band_peak)
        .collect()
}

/// Full-track three-band overview: stack bass, mids and highs upward from the
/// bottom edge, with one shared scale so quieter passages retain their shape.
pub fn half_bands(p: &Painter, rect: Rect, bands: &Bands, range: std::ops::Range<f64>) {
    let bins = (rect.width() * p.ctx().pixels_per_point())
        .ceil()
        .clamp(1.0, 4096.0) as usize;
    let span = range.end - range.start;
    let columns: Vec<_> = (0..bins)
        .map(|i| {
            bands
                .peak(
                    range.start + span * i as f64 / bins as f64,
                    range.start + span * (i + 1) as f64 / bins as f64,
                )
                .unwrap_or_default()
        })
        .collect();
    half_band_columns(p, rect, &columns);
}

/// Shared half-waveform renderer for Inspect and cached player overviews.
pub fn half_band_columns(p: &Painter, rect: Rect, columns: &[BandEntry]) {
    let peak = columns
        .iter()
        .map(|e| u16::from(e.bass) + u16::from(e.mid) + u16::from(e.high))
        .max()
        .unwrap_or(0)
        .max(1);
    let scale = rect.height() / f32::from(peak);
    for (i, e) in columns.iter().enumerate() {
        let left = rect.left() + i as f32 * rect.width() / columns.len() as f32;
        let right = rect.left() + (i + 1) as f32 * rect.width() / columns.len() as f32;
        let mut bottom = rect.bottom();
        for (amplitude, color) in [(e.bass, BASS), (e.mid, MID), (e.high, HIGH)] {
            let top = bottom - f32::from(amplitude) * scale;
            if amplitude > 0 {
                p.rect_filled(
                    Rect::from_min_max(pos2(left, top), pos2(right, bottom)),
                    0,
                    color,
                );
            }
            bottom = top;
        }
    }
}

/// Paint one waveform column. Callers retain grid, cue and playhead
/// overlays, so changing the palette never changes timing or interaction.
pub fn column(
    p: &Painter,
    rect: Rect,
    style: Style,
    overview: Overview,
    spectral: Option<WaveEntry>,
    bands: Option<BandEntry>,
) {
    let paint = |height: f32, color: Color32| {
        if height > 0.0 {
            let height = rect.height() * height.clamp(0.0, 1.0);
            let bottom = match overview {
                Overview::Half => rect.bottom(),
                Overview::Full => rect.center().y + height * 0.5,
            };
            p.rect_filled(
                Rect::from_min_max(
                    pos2(rect.left(), bottom - height),
                    pos2(rect.right(), bottom),
                ),
                0,
                color,
            );
        }
    };
    if style.uses_bands(bands.is_some()) {
        let e = bands.unwrap();
        // Independent amplitudes overlap from tallest to shortest, revealing
        // blue bass, amber mids and pale highs, as on the three-band display.
        let mut layers = [(e.bass, BASS), (e.mid, MID), (e.high, HIGH)];
        layers.sort_by_key(|(height, _)| std::cmp::Reverse(*height));
        for (height, color) in layers {
            paint(f32::from(height) / 255.0, color);
        }
    } else if let Some(e) = spectral {
        paint(f32::from(e.h) / 31.0, spectral_color(e, style));
    }
}

fn spectral_color(e: WaveEntry, style: Style) -> Color32 {
    if style == Style::Blue {
        let sum = f32::from(e.r.min(7)) + f32::from(e.g.min(7)) + f32::from(e.b.min(7));
        let treble = f32::from(e.b.min(7)) / sum.max(1.0) * 0.85;
        Color32::from_rgb(
            (40.0 + 215.0 * treble) as u8,
            (120.0 + 135.0 * treble) as u8,
            (235.0 + 20.0 * treble) as u8,
        )
    } else {
        Color32::from_rgb(
            50 + e.r.min(7) * 26,
            50 + e.g.min(7) * 26,
            50 + e.b.min(7) * 26,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsampling_preserves_each_band_and_clips_track_edges() {
        let bands = Bands {
            sample_rate: 150,
            entries: vec![
                BandEntry {
                    bass: 250,
                    mid: 10,
                    high: 30,
                },
                BandEntry {
                    bass: 10,
                    mid: 240,
                    high: 20,
                },
                BandEntry {
                    bass: 20,
                    mid: 30,
                    high: 230,
                },
            ],
            ..Default::default()
        };
        assert_eq!(
            bands.peak(-1.0, 0.02),
            Some(BandEntry {
                bass: 250,
                mid: 240,
                high: 230
            })
        );
        assert_eq!(bands.peak(0.014, 0.02), Some(bands.entries[2]));
        assert!(bands.peak(-2.0, -1.0).is_none());
        assert!(bands.peak(1.0, 2.0).is_none());
        assert!(bands.peak(f64::NAN, 2.0).is_none());
        assert!(summarize_bands(&vec![bands.entries[0]; 30_000]).len() <= 1024);
    }

    #[test]
    fn styles_render_distinct_palettes_and_missing_bands_fall_back_to_rgb() {
        use eframe::egui::{Context, Shape};
        let draw_with_shape = |style, bands, h, overview| {
            let ctx = Context::default();
            let mut output = ctx.run_ui(Default::default(), |ui| {
                column(
                    &ui.ctx().debug_painter(),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 100.0)),
                    style,
                    overview,
                    Some(WaveEntry {
                        r: 7,
                        g: 3,
                        b: 1,
                        h,
                    }),
                    bands,
                );
            });
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|s| match s.shape {
                    Shape::Rect(r) => Some((r.fill, r.rect)),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let draw = |style, bands, h| draw_with_shape(style, bands, h, Overview::Full);
        for style in [Style::Rgb, Style::Blue] {
            let full = draw(style, None, 15);
            let half = draw_with_shape(style, None, 15, Overview::Half);
            assert_eq!(full[0].0, half[0].0);
            assert!((full[0].1.height() - half[0].1.height()).abs() < 0.001);
            assert_eq!(full[0].1.center().y, 50.0);
            assert_eq!(half[0].1.bottom(), 100.0);
        }
        let bands = Some(BandEntry {
            bass: 255,
            mid: 128,
            high: 32,
        });
        let rgb = draw(Style::Rgb, bands, 31);
        let blue = draw(Style::Blue, bands, 31);
        let three = draw(Style::ThreeBand, bands, 31);
        assert_ne!(rgb[0].0, blue[0].0);
        assert_eq!(rgb[0].1, blue[0].1);
        assert_eq!(three.len(), 3);
        assert_eq!(three[0].1.height(), 100.0);
        assert!(three[1].1.height() > three[2].1.height());
        assert_eq!(draw(Style::Auto, bands, 31), three);
        assert_eq!(draw(Style::Auto, None, 31), rgb);
        assert_eq!(draw(Style::ThreeBand, None, 31), rgb);
        assert!(draw(Style::Blue, None, 0).is_empty());
        assert_eq!(draw(Style::Rgb, None, 255)[0].1.height(), 100.0);
    }
}
