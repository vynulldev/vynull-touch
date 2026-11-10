// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{cues, model::Cue};
use eframe::egui::*;
use std::ops::Range;

#[derive(Clone, Copy)]
pub enum Edge {
    Top,
    Bottom,
}

struct Marker {
    number: u16,
    x: f32,
    color: Color32,
    flag: [Pos2; 3],
    label: Option<Rect>,
}

fn layout(rect: Rect, range: Range<f64>, cues: &[Cue], edge: Edge) -> Vec<Marker> {
    if !rect.is_finite()
        || rect.width() < 8.0
        || rect.height() < 12.0
        || !range.start.is_finite()
        || !range.end.is_finite()
        || range.end <= range.start
    {
        return Vec::new();
    }
    let compact = rect.height() < 50.0;
    let flag_height = if compact { 4.0 } else { 6.0 };
    let label_size = if compact {
        vec2(12.0, 14.0)
    } else {
        vec2(18.0, 18.0)
    };
    let mut visible: Vec<_> = cues
        .iter()
        .filter(|cue| {
            (1..=8).contains(&cue.number)
                && matches!(cue.kind.as_str(), "cue" | "loop")
                && (range.start..=range.end).contains(&(f64::from(cue.time_ms) / 1000.0))
        })
        .collect();
    visible.sort_by_key(|cue| (cue.time_ms, cue.number));
    let x_at = |cue: &Cue| {
        let phase = (f64::from(cue.time_ms) / 1000.0 - range.start) / (range.end - range.start);
        (rect.left() + phase as f32 * rect.width()).clamp(rect.left() + 1.0, rect.right() - 1.0)
    };
    let positions: Vec<_> = visible.iter().map(|cue| x_at(cue)).collect();
    let mut used = [false; 8];
    let mut label_right = f32::NEG_INFINITY;
    visible
        .into_iter()
        .filter_map(|cue| {
            let slot = usize::from(cue.number - 1);
            if used[slot] {
                return None;
            }
            used[slot] = true;
            // Only inset the end points for the stroke; interior positions retain
            // exactly the waveform's time mapping.
            let x = x_at(cue);
            let (y, tip, label_y) = match edge {
                Edge::Top => (
                    rect.top() + 1.0,
                    rect.top() + 1.0 + flag_height,
                    rect.top() + 1.0,
                ),
                Edge::Bottom => (
                    rect.bottom() - 1.0,
                    rect.bottom() - 1.0 - flag_height,
                    rect.bottom() - 1.0 - label_size.y,
                ),
            };
            let flag = [
                pos2((x - flag_height).max(rect.left() + 1.0), y),
                pos2((x + flag_height).min(rect.right() - 1.0), y),
                pos2(x, tip),
            ];
            let label = if rect.width() >= label_size.x + 2.0 && rect.height() >= label_size.y + 2.0
            {
                let left = if x + flag_height + 2.0 + label_size.x < rect.right() {
                    x + flag_height + 2.0
                } else {
                    x - flag_height - 2.0 - label_size.x
                }
                .clamp(rect.left() + 1.0, rect.right() - label_size.x - 1.0);
                if left >= label_right + 2.0
                    && !positions.iter().any(|other| {
                        *other != x
                            && *other + flag_height >= left
                            && *other - flag_height <= left + label_size.x
                    })
                {
                    let label = Rect::from_min_size(pos2(left, label_y), label_size);
                    label_right = label.right();
                    Some(label)
                } else {
                    None
                }
            } else {
                None
            };
            Some(Marker {
                number: cue.number,
                x,
                color: cues::color(cue.color_id),
                flag,
                label,
            })
        })
        .collect()
}

/// Read-only overlays: retain the waveform's tap/drag behavior. A black outline
/// separates each cue from every palette, and crowded cues keep their flags and
/// timing lines even when there is no room for another letter badge.
pub fn hot_cues(p: &Painter, rect: Rect, range: Range<f64>, cues: &[Cue], edge: Edge) {
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    let markers = layout(rect, range, cues, edge);
    let width = if rect.height() < 50.0 { 1.5 } else { 2.0 };
    for marker in &markers {
        p.vline(
            marker.x,
            rect.y_range(),
            Stroke::new(width + 2.0, Color32::BLACK),
        );
        p.vline(marker.x, rect.y_range(), Stroke::new(width, marker.color));
    }
    for marker in markers {
        p.add(Shape::convex_polygon(
            marker.flag.to_vec(),
            marker.color,
            Stroke::new(1.0, Color32::BLACK),
        ));
        if let Some(label) = marker.label {
            p.rect_filled(label, 2, marker.color);
            p.rect_stroke(
                label,
                2,
                Stroke::new(1.0, Color32::BLACK),
                StrokeKind::Inside,
            );
            p.text(
                label.center(),
                Align2::CENTER_CENTER,
                cues::slot(marker.number),
                FontId::monospace(if rect.height() < 50.0 { 11.0 } else { 13.0 }),
                Color32::BLACK,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(number: u16, time_ms: u32) -> Cue {
        Cue {
            number,
            time_ms,
            kind: "cue".into(),
            color_id: 0x2a,
            ..Default::default()
        }
    }

    #[test]
    fn cue_markers_map_exact_times_clip_zoom_edges_and_include_hot_loops() {
        let rect = Rect::from_min_size(pos2(20.0, 40.0), vec2(300.0, 100.0));
        let cues = [
            cue(1, 0),
            cue(2, 25_125),
            cue(3, 100_000),
            cue(4, 100_001),
            cue(9, 30_000),
            Cue {
                kind: "loop".into(),
                loop_ms: 60_000,
                ..cue(8, 50_000)
            },
        ];
        for edge in [Edge::Top, Edge::Bottom] {
            let markers = layout(rect, 0.0..100.0, &cues, edge);
            assert_eq!(
                markers.iter().map(|m| m.number).collect::<Vec<_>>(),
                [1, 2, 8, 3]
            );
            assert!((markers[1].x - (rect.left() + 75.375)).abs() < 0.001);
            assert_eq!(markers[0].x, rect.left() + 1.0);
            assert_eq!(markers[3].x, rect.right() - 1.0);
            assert_eq!(markers[1].color, cues::color(0x2a));
            for marker in markers {
                assert!(marker.flag.iter().all(|&point| rect.contains(point)));
                assert!(marker.label.is_none_or(|label| rect.contains_rect(label)));
            }
            let zoom = layout(rect, 25.125..50.0, &cues, edge);
            assert_eq!(zoom.iter().map(|m| m.number).collect::<Vec<_>>(), [2, 8]);
            assert_eq!(zoom[0].x, rect.left() + 1.0);
            assert_eq!(zoom[1].x, rect.right() - 1.0);
        }
        for range in [0.0..0.0, 2.0..1.0, f64::NAN..10.0, 0.0..f64::INFINITY] {
            assert!(layout(rect, range, &cues, Edge::Top).is_empty());
        }
    }

    #[test]
    fn crowded_cue_badges_do_not_cover_other_flags_or_each_other() {
        let cues = [
            cue(1, 0),
            cue(2, 100),
            cue(3, 110),
            cue(4, 300),
            cue(5, 500),
            cue(6, 501),
            cue(7, 990),
            cue(8, 1000),
        ];
        for size in [
            vec2(8.0, 12.0),
            vec2(76.0, 42.0),
            vec2(668.0, 28.0),
            vec2(1000.0, 176.0),
        ] {
            let rect = Rect::from_min_size(pos2(5.0, 7.0), size);
            for edge in [Edge::Top, Edge::Bottom] {
                let markers = layout(rect, 0.0..1.0, &cues, edge);
                assert_eq!(
                    markers.len(),
                    8,
                    "crowding must not discard cue timing lines"
                );
                for marker in &markers {
                    if let Some(label) = marker.label {
                        assert!(rect.contains_rect(label));
                        for other in &markers {
                            if other.x == marker.x {
                                continue;
                            }
                            assert!(!label.intersects(Rect::from_points(&other.flag)));
                            if let Some(other_label) = other.label {
                                assert!(!label.intersects(other_label));
                            }
                        }
                    }
                }
            }
        }
    }
}
