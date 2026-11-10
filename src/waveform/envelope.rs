// SPDX-License-Identifier: GPL-3.0-or-later
use super::{BASS, BandEntry, HIGH, MID, Style, WaveEntry, spectral_color};
use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};
use eframe::egui::{Color32, Painter, Rect, Shape, Vec2, pos2, vec2};

#[derive(Clone, Copy, Default)]
pub struct Column {
    pub spectral: Option<WaveEntry>,
    pub bands: Option<BandEntry>,
}

#[derive(Clone, Copy, Default)]
struct Layer {
    amplitude: f32,
    color: Color32,
}

fn layers(columns: &[Column], style: Style, soften: bool) -> Vec<[Layer; 3]> {
    let mut samples: Vec<_> = columns
        .iter()
        .map(|column| {
            let mut layers = [Layer::default(); 3];
            if style.uses_bands(column.bands.is_some()) {
                let bands = column.bands.unwrap();
                for (layer, (height, color)) in layers.iter_mut().zip([
                    (bands.bass, BASS),
                    (bands.mid, MID),
                    (bands.high, HIGH),
                ]) {
                    *layer = Layer {
                        amplitude: f32::from(height) / 255.0,
                        color,
                    };
                }
            } else if let Some(entry) = column.spectral {
                layers[0] = Layer {
                    amplitude: (f32::from(entry.h) / 31.0).min(1.0),
                    color: spectral_color(entry, style),
                };
            }
            layers
        })
        .collect();
    if soften && samples.len() > 2 {
        let original = samples.clone();
        for i in 1..samples.len() - 1 {
            for band in 0..3 {
                // Only soften subpixel valleys next to a peak. Never reduce a
                // transient, move its time, or spread it past the next pixel.
                if original[i][band].amplitude > 0.0
                    && original[i - 1][band].amplitude > 0.0
                    && original[i + 1][band].amplitude > 0.0
                {
                    let average = (original[i - 1][band].amplitude
                        + 2.0 * original[i][band].amplitude
                        + original[i + 1][band].amplitude)
                        * 0.25;
                    samples[i][band].amplitude = original[i][band].amplitude.max(average);
                }
            }
        }
    }
    // Match the existing three-band palette: shorter bands overlay taller ones.
    for sample in &mut samples {
        sample.sort_by(|a, b| b.amplitude.total_cmp(&a.amplitude));
    }
    samples
}

/// Join the peak columns into continuous, antialiased envelopes. One mesh per
/// layer avoids feathering internal column seams and keeps Pi rendering bounded
/// by display resolution. Grid/cue/playhead overlays remain separate and sharp.
pub fn paint(p: &Painter, rect: Rect, style: Style, columns: &[Column], soften: bool) {
    if columns.is_empty() || !rect.is_finite() || rect.width() <= 0.0 || rect.height() <= 0.0 {
        return;
    }
    let samples = layers(columns, style, soften);
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let feather = 0.5 / p.ctx().pixels_per_point();
    let count = samples.len();
    for layer in 0..3 {
        if samples.iter().all(|sample| sample[layer].amplitude == 0.0) {
            continue;
        }
        let mut mesh = Mesh::default();
        // Duplicate the first/last sample at the rectangle's edge. Interior
        // samples are at bin centers, keeping the original time mapping.
        let points: Vec<_> = (0..count + 2)
            .map(|i| {
                let index = i.saturating_sub(1).min(count - 1);
                let x = match i {
                    0 => rect.left(),
                    n if n == count + 1 => rect.right(),
                    _ => rect.left() + (index as f32 + 0.5) * rect.width() / count as f32,
                };
                (x, samples[index][layer])
            })
            .collect();
        for (i, &(x, sample)) in points.iter().enumerate() {
            let half = sample.amplitude * rect.height() * 0.5;
            let previous = points[i.saturating_sub(1)];
            let next = points[(i + 1).min(points.len() - 1)];
            let slope = -(next.1.amplitude - previous.1.amplitude) * rect.height() * 0.5
                / (next.0 - previous.0).max(f32::EPSILON);
            let normal = vec2(slope, -1.0).normalized() * feather;
            let inset = Vec2::new(normal.x, normal.y.max(-half));
            let top = pos2(x, rect.center().y - half);
            let bottom = pos2(x, rect.center().y + half);
            let color = sample.color.gamma_multiply((half / feather).min(1.0));
            for (pos, color) in [
                (top + normal, Color32::TRANSPARENT),
                (top - inset, color),
                (bottom - vec2(inset.x, -inset.y), color),
                (bottom + vec2(normal.x, -normal.y), Color32::TRANSPARENT),
            ] {
                mesh.vertices.push(Vertex {
                    pos,
                    uv: WHITE_UV,
                    color,
                });
            }
            if i > 0 {
                let previous = (i as u32 - 1) * 4;
                let current = i as u32 * 4;
                for strip in 0..3 {
                    mesh.indices.extend_from_slice(&[
                        previous + strip,
                        current + strip,
                        previous + strip + 1,
                        previous + strip + 1,
                        current + strip,
                        current + strip + 1,
                    ]);
                }
            }
        }
        p.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Context;

    #[test]
    fn waveform_smoothing_preserves_peak_time_height_and_silent_gaps() {
        let columns: Vec<_> = [3, 3, 31, 3, 3, 0, 31, 0]
            .into_iter()
            .map(|h| Column {
                spectral: Some(WaveEntry {
                    h,
                    r: 7,
                    g: 3,
                    b: 1,
                }),
                bands: None,
            })
            .collect();
        for style in [Style::Rgb, Style::Blue] {
            let raw = layers(&columns, style, false);
            let smooth = layers(&columns, style, true);
            assert_eq!(smooth[2][0].amplitude, 1.0);
            assert_eq!(smooth[6][0].amplitude, 1.0);
            assert!(smooth[1][0].amplitude > raw[1][0].amplitude);
            for i in [0, 4, 5, 7] {
                assert_eq!(smooth[i][0].amplitude, raw[i][0].amplitude);
            }
            for (raw, smooth) in raw.iter().zip(&smooth) {
                assert!(smooth[0].amplitude >= raw[0].amplitude);
                assert!(smooth[0].amplitude <= 1.0);
                assert_eq!(smooth[0].color, raw[0].color);
            }
        }
    }

    #[test]
    fn waveform_envelopes_have_antialiased_edges_valid_meshes_and_bounded_geometry() {
        let columns: Vec<_> = (0..20)
            .map(|i| Column {
                spectral: Some(WaveEntry {
                    h: (i % 5) * 7,
                    r: 2,
                    g: 5,
                    b: 7,
                }),
                bands: Some(BandEntry {
                    bass: i * 12,
                    mid: 255 - i * 12,
                    high: i * 3,
                }),
            })
            .collect();
        for ppp in [1.0, 2.0] {
            for style in Style::ALL {
                let ctx = Context::default();
                ctx.set_pixels_per_point(ppp);
                let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(300.0, 100.0));
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    paint(ui.painter(), rect, style, &columns, true);
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
                    assert_eq!(mesh.vertices.len(), 4 * (columns.len() + 2));
                    assert!(
                        mesh.indices
                            .iter()
                            .all(|&i| (i as usize) < mesh.vertices.len())
                    );
                    assert!(
                        mesh.vertices
                            .iter()
                            .any(|v| v.color == Color32::TRANSPARENT)
                    );
                    assert!(mesh.vertices.iter().any(|v| v.color.a() == 255));
                    assert!(mesh.vertices.iter().all(
                        |v| v.pos.is_finite() && rect.expand(0.5 / ppp + 0.001).contains(v.pos)
                    ));
                }
            }
        }
        let ctx = Context::default();
        let mut output = ctx.run_ui(Default::default(), |ui| {
            paint(
                ui.painter(),
                Rect::from_min_size(pos2(0.0, 0.0), vec2(20.0, 30.0)),
                Style::Auto,
                &[Column::default(); 20],
                true,
            );
        });
        output.textures_delta.clear();
        assert!(
            !output
                .shapes
                .iter()
                .any(|s| matches!(s.shape, Shape::Mesh(_)))
        );
    }
}
