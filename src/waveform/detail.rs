// SPDX-License-Identifier: GPL-3.0-or-later
//! Zoom-aware detail rendering shared by Inspect and Players.
use super::{
    Bands, Style, WaveEntry,
    envelope::{self, Column, Layer},
};
use eframe::egui::{Color32, Painter, Rect};
use std::ops::Range;

enum Source<'a> {
    Spectral(&'a [WaveEntry], u32),
    Bands(&'a Bands),
}

impl Source<'_> {
    fn rate(&self) -> f64 {
        f64::from(match self {
            Self::Spectral(_, rate) => *rate,
            Self::Bands(bands) => bands.sample_rate,
        })
    }

    fn len(&self) -> usize {
        match self {
            Self::Spectral(entries, _) => entries.len(),
            Self::Bands(bands) => bands.entries.len(),
        }
    }

    fn at(&self, index: i64) -> Column {
        let index = index.clamp(0, self.len().saturating_sub(1) as i64) as usize;
        match self {
            Self::Spectral(entries, _) => Column {
                spectral: entries.get(index).copied(),
                bands: None,
            },
            Self::Bands(bands) => Column {
                spectral: None,
                bands: bands.entries.get(index).copied(),
            },
        }
    }

    fn peak(&self, start: f64, end: f64) -> Column {
        match self {
            Self::Spectral(entries, _) => {
                let first = (start.max(0.0) * self.rate()).floor() as usize;
                let last = (end.max(0.0) * self.rate()).ceil() as usize;
                Column {
                    spectral: entries
                        .get(first.min(entries.len())..last.min(entries.len()))
                        .and_then(|entries| entries.iter().max_by_key(|e| e.h))
                        .copied(),
                    bands: None,
                }
            }
            Self::Bands(bands) => Column {
                spectral: None,
                bands: bands.peak(start, end),
            },
        }
    }
}

/// Use pixel peaks when reducing analysis, and shape-preserving interpolation
/// when magnifying it. The latter retains fractional heights instead of rounding
/// back to the analysis format's integer amplitudes. No audio is decoded here.
pub fn paint(
    p: &Painter,
    rect: Rect,
    style: Style,
    spectral: &[WaveEntry],
    sample_rate: u32,
    bands: &Bands,
    range: Range<f64>,
) {
    let span = range.end - range.start;
    if !rect.is_finite()
        || rect.width() <= 0.0
        || rect.height() <= 0.0
        || !range.start.is_finite()
        || !range.end.is_finite()
        || !span.is_finite()
        || span <= 0.0
    {
        return;
    }
    let source = if style.uses_bands(bands.available()) {
        Source::Bands(bands)
    } else {
        Source::Spectral(spectral, sample_rate)
    };
    if source.len() == 0 || source.rate() == 0.0 {
        return;
    }
    let bins = (rect.width() * p.ctx().pixels_per_point())
        .ceil()
        .clamp(1.0, 4096.0) as usize;
    if span * source.rate() > bins as f64 {
        let columns: Vec<_> = (0..bins)
            .map(|i| {
                source.peak(
                    range.start + span * i as f64 / bins as f64,
                    range.start + span * (i + 1) as f64 / bins as f64,
                )
            })
            .collect();
        envelope::paint(p, rect, style, &columns, true);
    } else {
        let points = interpolated(&source, style, range, bins)
            .into_iter()
            .map(|(fraction, layers)| (rect.left() + fraction as f32 * rect.width(), layers))
            .collect::<Vec<_>>();
        envelope::paint_samples(p, rect, &points);
    }
}

fn interpolated(
    source: &Source<'_>,
    style: Style,
    range: Range<f64>,
    bins: usize,
) -> Vec<(f64, [Layer; 3])> {
    let span = range.end - range.start;
    let rate = source.rate();
    let start = range.start.max(0.0);
    let end = range.end.min(source.len() as f64 / rate);
    if start >= end {
        return vec![];
    }
    // Blank space before/after the track stays blank. Extend the first/last
    // analysis point only to the edge of its own sample interval.
    let mut times = vec![start, end];
    for i in 0..bins {
        let time = range.start + (i as f64 + 0.5) * span / bins as f64;
        if time > start && time < end {
            times.push(time);
        }
    }
    // Include the original sample centers as well as pixel centers. Even a
    // narrow transient keeps its exact height and time as the view scrolls.
    let first = ((start * rate - 0.5).ceil().max(0.0) as usize).min(source.len());
    let last = ((end * rate - 0.5).floor().max(0.0) as usize).min(source.len() - 1);
    for i in first..=last {
        let time = (i as f64 + 0.5) / rate;
        if time >= start && time <= end {
            times.push(time);
        }
    }
    times.sort_by(f64::total_cmp);
    times.dedup();
    times
        .into_iter()
        .map(|time| {
            let position = time * rate - 0.5;
            let index = position.floor() as i64;
            let t = (position - index as f64) as f32;
            let neighbors = [index - 1, index, index + 1, index + 2]
                .map(|i| envelope::sample(source.at(i), style));
            let mut layers = [Layer::default(); 3];
            for band in 0..3 {
                layers[band] = Layer {
                    amplitude: curve(neighbors.map(|layers| layers[band].amplitude), t),
                    color: blend(neighbors[1][band].color, neighbors[2][band].color, t),
                };
            }
            // Interpolate each frequency band independently before layering them.
            layers.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
            ((time - range.start) / span, layers)
        })
        .collect()
}

/// Monotone cubic Hermite interpolation: flat at extrema, no overshoot or
/// ringing across quiet passages, and exact at each original analysis point.
fn curve([a, b, c, d]: [f32; 4], t: f32) -> f32 {
    let tangent = |left: f32, right: f32| {
        if left * right <= 0.0 {
            0.0
        } else {
            2.0 * left * right / (left + right)
        }
    };
    let m0 = tangent(b - a, c - b);
    let m1 = tangent(c - b, d - c);
    let t2 = t * t;
    let t3 = t2 * t;
    ((2.0 * t3 - 3.0 * t2 + 1.0) * b
        + (t3 - 2.0 * t2 + t) * m0
        + (-2.0 * t3 + 3.0 * t2) * c
        + (t3 - t2) * m1)
        .clamp(b.min(c), b.max(c))
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let a = a.to_array();
    let b = b.to_array();
    let rgba = std::array::from_fn::<_, 4, _>(|i| {
        (f32::from(a[i]) + (f32::from(b[i]) - f32::from(a[i])) * t).round() as u8
    });
    Color32::from_rgba_premultiplied(rgba[0], rgba[1], rgba[2], rgba[3])
}

#[cfg(test)]
mod tests;
