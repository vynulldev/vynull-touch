// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::waveform::{BASS, BandEntry, HIGH, MID};
use eframe::egui::{Context, Shape, pos2, vec2};

#[test]
fn curves_pass_through_samples_without_overshoot_or_ringing_in_silence() {
    for values in [
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.8, 0.0, 0.0, 1.0],
        [0.1, 0.2, 0.6, 0.9],
    ] {
        assert_eq!(curve(values, 0.0), values[1]);
        assert!((curve(values, 1.0) - values[2]).abs() < 1e-6);
        let mut previous = values[1];
        for step in 0..=100 {
            let value = curve(values, step as f32 / 100.0);
            assert!((values[1].min(values[2])..=values[1].max(values[2])).contains(&value));
            if values[1] <= values[2] {
                assert!(value >= previous);
            } else {
                assert!(value <= previous);
            }
            previous = value;
        }
    }
}

#[test]
fn magnified_waveforms_keep_exact_peaks_fractional_heights_and_absolute_timing() {
    let entries: Vec<_> = [0, 31, 0, 0, 15, 3]
        .into_iter()
        .enumerate()
        .map(|(i, h)| WaveEntry {
            h,
            r: i as u8,
            g: 3,
            b: 7,
        })
        .collect();
    let source = Source::Spectral(&entries, 150);
    for style in [Style::Rgb, Style::Blue, Style::Auto, Style::ThreeBand] {
        for range in [-0.02..0.07, 0.001..0.039, 0.008..0.022] {
            let samples = interpolated(&source, style, range.clone(), 300);
            let span = range.end - range.start;
            assert!(samples.len() <= 300 + entries.len() + 2);
            for (index, entry) in entries.iter().enumerate() {
                let time = (index as f64 + 0.5) / 150.0;
                if range.contains(&time) {
                    let (_, layers) = samples
                        .iter()
                        .find(|(x, _)| (range.start + x * span - time).abs() < 1e-10)
                        .unwrap();
                    assert!((layers[0].amplitude - f32::from(entry.h) / 31.0).abs() < 1e-6);
                    assert_eq!(
                        layers[0].color,
                        crate::waveform::spectral_color(*entry, style)
                    );
                }
            }
            let mut fractional = 0;
            for (x, layers) in &samples {
                let time = range.start + x * span;
                assert!((-1e-10..=entries.len() as f64 / 150.0 + 1e-10).contains(&time));
                if (2.5 / 150.0..=3.5 / 150.0).contains(&time) {
                    assert_eq!(
                        layers[0].amplitude, 0.0,
                        "silent intervals must stay silent"
                    );
                }
                let height = layers[0].amplitude * 31.0;
                fractional += usize::from((height - height.round()).abs() > 0.01);
            }
            assert!(
                fractional > 20,
                "high zoom must not repeat integer-height columns"
            );
        }
    }
    assert!(interpolated(&source, Style::Rgb, -1.0..-0.5, 100).is_empty());
    assert!(interpolated(&source, Style::Rgb, 1.0..2.0, 100).is_empty());
}

#[test]
fn bands_interpolate_independently_and_reduction_keeps_transients() {
    let bands = Bands {
        sample_rate: 150,
        entries: vec![
            BandEntry {
                bass: 255,
                mid: 100,
                high: 20,
            },
            BandEntry {
                bass: 0,
                mid: 200,
                high: 80,
            },
            BandEntry::default(),
        ],
        ..Default::default()
    };
    let source = Source::Bands(&bands);
    let samples = interpolated(&source, Style::ThreeBand, 0.0..0.02, 300);
    let (_, layers) = samples
        .iter()
        .find(|(fraction, _)| (*fraction * 0.02 - 1.0 / 150.0).abs() < 0.00004)
        .unwrap();
    for (color, low, high) in [
        (BASS, 0.0, 1.0),
        (MID, 100.0 / 255.0, 200.0 / 255.0),
        (HIGH, 20.0 / 255.0, 80.0 / 255.0),
    ] {
        let amplitude = layers
            .iter()
            .find(|layer| layer.color == color)
            .unwrap()
            .amplitude;
        assert!(amplitude > low && amplitude < high);
    }
    assert_eq!(
        source.peak(0.0, 0.02).bands,
        Some(BandEntry {
            bass: 255,
            mid: 200,
            high: 80
        })
    );
    let entries = [
        WaveEntry {
            h: 2,
            ..Default::default()
        },
        WaveEntry {
            h: 31,
            ..Default::default()
        },
        WaveEntry {
            h: 4,
            ..Default::default()
        },
    ];
    let source = Source::Spectral(&entries, 150);
    assert_eq!(source.peak(0.0, 0.02).spectral.unwrap().h, 31);
    assert_eq!(source.peak(0.014, 0.02).spectral.unwrap().h, 4);
    assert!(source.peak(-2.0, -1.0).spectral.is_none());
    assert!(source.peak(1.0, 2.0).spectral.is_none());
}

#[test]
fn detail_meshes_are_clipped_finite_and_bounded_at_both_sampling_scales() {
    let entries: Vec<_> = (0..1500)
        .map(|i| WaveEntry {
            h: (i % 32) as u8,
            r: 7,
            g: 3,
            b: 1,
        })
        .collect();
    let bands = Bands {
        sample_rate: 150,
        entries: (0..1500)
            .map(|i| BandEntry {
                bass: (i % 256) as u8,
                mid: (i % 100) as u8,
                high: 30,
            })
            .collect(),
        ..Default::default()
    };
    for ppp in [1.0, 2.0] {
        for style in Style::ALL {
            for range in [-0.2..0.8, 0.0..10.0, 9.8..10.5] {
                let ctx = Context::default();
                ctx.set_pixels_per_point(ppp);
                let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(300.0, 100.0));
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    paint(
                        ui.painter(),
                        rect,
                        style,
                        &entries,
                        150,
                        &bands,
                        range.clone(),
                    )
                });
                output.textures_delta.clear();
                let meshes: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Mesh(mesh) => Some((shape.clip_rect, mesh)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(meshes.len(), if style.uses_bands(true) { 3 } else { 1 });
                for (clip, mesh) in meshes {
                    assert!(rect.contains_rect(clip));
                    assert!(mesh.vertices.len() <= 4 * (2 * (300.0 * ppp) as usize + 4));
                    assert!(
                        mesh.indices
                            .iter()
                            .all(|&i| (i as usize) < mesh.vertices.len())
                    );
                    assert!(mesh.vertices.iter().all(
                        |v| v.pos.is_finite() && rect.expand(0.5 / ppp + 0.001).contains(v.pos)
                    ));
                    assert!(
                        mesh.vertices
                            .iter()
                            .any(|v| v.color == Color32::TRANSPARENT)
                    );
                }
            }
        }
    }
    let ctx = Context::default();
    let mut output = ctx.run_ui(Default::default(), |ui| {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 50.0));
        for range in [1.0..0.0, 0.0..0.0, f64::NAN..1.0, 1.0..f64::INFINITY] {
            paint(ui.painter(), rect, Style::Rgb, &entries, 150, &bands, range);
        }
        paint(
            ui.painter(),
            rect,
            Style::Rgb,
            &entries,
            0,
            &Bands::default(),
            0.0..1.0,
        );
        paint(
            ui.painter(),
            rect,
            Style::Auto,
            &[],
            150,
            &Bands::default(),
            0.0..1.0,
        );
    });
    output.textures_delta.clear();
    assert!(
        !output
            .shapes
            .iter()
            .any(|shape| matches!(shape.shape, Shape::Mesh(_)))
    );
}
