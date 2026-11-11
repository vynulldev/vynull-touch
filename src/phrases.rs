// SPDX-License-Identifier: GPL-3.0-or-later
use eframe::egui::*;
use serde::Deserialize;
use std::{borrow::Cow, ops::Range};

pub const STRIP_HEIGHT: f32 = 18.0;

/// Phrase colours by label, matching Vynull's web client (PHRASE_COLORS in
/// drawer.js). Plain UP, from Vynull's own analysis, takes UP 1's colour.
const COLORS: [(&str, u32); 21] = [
    ("INTRO", 0xe14646),
    ("INTRO 1", 0xc80000),
    ("INTRO 2", 0xc83200),
    ("UP", 0x8c32ff),
    ("UP 1", 0x8c32ff),
    ("UP 2", 0x6932ff),
    ("UP 3", 0x5a32ff),
    ("DOWN", 0x9b732d),
    ("CHORUS", 0x78c37d),
    ("CHORUS 1", 0x0faa00),
    ("CHORUS 2", 0x00aa7d),
    ("OUTRO", 0x677587),
    ("OUTRO 1", 0x4879af),
    ("OUTRO 2", 0x5f87af),
    ("BRIDGE", 0xe1d741),
    ("VERSE 1", 0x506eff),
    ("VERSE 2", 0x5055ff),
    ("VERSE 3", 0x6450ff),
    ("VERSE 4", 0x7850ff),
    ("VERSE 5", 0x7e48e5),
    ("VERSE 6", 0xa050ff),
];

/// Absolute audio times supplied by Vynull, independent of subsequent grid edits.
#[derive(Clone, Debug, Deserialize)]
pub struct Phrase {
    pub kind: u16,
    #[serde(default)]
    pub name: String,
    pub start_ms: u32,
    pub end_ms: u32,
}

impl Phrase {
    pub fn start(&self) -> f64 {
        f64::from(self.start_ms) / 1000.0
    }

    fn end(&self) -> f64 {
        f64::from(self.end_ms) / 1000.0
    }

    pub fn label(&self) -> Cow<'_, str> {
        if !self.name.trim().is_empty() {
            return self.name.trim().into();
        }
        match self.kind {
            1 => "INTRO".into(),
            2 => "UP".into(),
            3 => "DOWN".into(),
            5 => "CHORUS".into(),
            6 => "OUTRO".into(),
            kind => format!("SECTION {kind}").into(),
        }
    }

    /// The label's colour, as Vynull's web client paints it; neutral for a
    /// label it doesn't know.
    fn color(&self) -> Color32 {
        let label = self.label();
        COLORS
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&label))
            .map_or(Color32::from_rgb(147, 157, 168), |&(_, hex)| {
                Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
            })
    }

    /// Black or white, whichever contrasts more with the colour (WCAG relative
    /// luminance, as the web client picks).
    fn text_color(&self) -> Color32 {
        let linear = |c: u8| {
            let c = f32::from(c) / 255.0;
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let c = self.color();
        let luminance = 0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b());
        if luminance > 0.179 {
            Color32::from_black_alpha(204)
        } else {
            Color32::WHITE
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Sections(Vec<Phrase>);

impl Sections {
    pub fn new(mut phrases: Vec<Phrase>) -> Result<Self, &'static str> {
        if phrases.iter().any(|p| p.end_ms < p.start_ms)
            || phrases
                .windows(2)
                .any(|pair| pair[0].end_ms > pair[1].start_ms)
        {
            return Err("Invalid phrase section timing");
        }
        // The API may return an empty interval when it clips a leading section.
        phrases.retain(|p| p.end_ms > p.start_ms);
        Ok(Self(phrases))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Sections use half-open intervals: a boundary belongs to the new phrase.
    /// Preserve unclassified lead-in, gaps and the tail after the final section.
    pub fn at(&self, seconds: f64) -> (Option<&Phrase>, Option<&Phrase>) {
        if !seconds.is_finite() || seconds < 0.0 {
            return (None, None);
        }
        let next = self.0.partition_point(|phrase| phrase.start() <= seconds);
        let current = next
            .checked_sub(1)
            .and_then(|index| self.0.get(index))
            .filter(|phrase| seconds < phrase.end());
        (current, self.0.get(next))
    }

    fn visible(&self, range: Range<f64>) -> impl Iterator<Item = &Phrase> {
        let valid = range.start.is_finite() && range.end.is_finite() && range.end > range.start;
        let first = self.0.partition_point(|p| p.end() <= range.start);
        self.0[first..]
            .iter()
            .take_while(move |p| valid && p.start() < range.end)
    }

    pub fn paint(&self, painter: &Painter, rect: Rect, range: Range<f64>) {
        if !rect.is_positive() || !rect.is_finite() {
            return;
        }
        let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
        p.rect_filled(rect, 0, crate::theme::CARD);
        let span = range.end - range.start;
        for phrase in self.visible(range.clone()) {
            let x = |time: f64| {
                rect.left() + ((time - range.start) / span).clamp(0.0, 1.0) as f32 * rect.width()
            };
            let segment =
                Rect::from_x_y_ranges(x(phrase.start())..=x(phrase.end()), rect.y_range());
            p.rect_filled(segment, 0, phrase.color());
            p.vline(
                segment.left(),
                segment.y_range(),
                Stroke::new(1.0, crate::theme::BG),
            );
            let text = phrase.text_color();
            let label = p.layout_no_wrap(
                phrase.label().into_owned(),
                FontId::proportional(11.0),
                text,
            );
            if label.size().x + 8.0 <= segment.width() {
                p.galley(
                    pos2(
                        segment.left() + 4.0,
                        segment.center().y - label.size().y / 2.0,
                    ),
                    label,
                    text,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phrase(kind: u16, start_ms: u32, end_ms: u32) -> Phrase {
        Phrase {
            kind,
            name: String::new(),
            start_ms,
            end_ms,
        }
    }

    #[test]
    fn sections_preserve_gaps_saved_timestamps_and_unknown_labels() {
        let sections = Sections::new(vec![
            phrase(1, 0, 0),
            phrase(1, 125, 32_125),
            phrase(5, 32_125, 64_125),
            Phrase {
                name: "BRIDGE".into(),
                ..phrase(99, 80_500, 100_500)
            },
        ])
        .unwrap();
        assert_eq!(sections.visible(0.0..101.0).count(), 3);
        assert_eq!(sections.visible(32.125..64.125).next().unwrap().kind, 5);
        assert!(sections.visible(64.125..80.5).next().is_none());
        let last = sections.visible(90.0..110.0).next().unwrap();
        assert_eq!(last.start_ms, 80_500);
        assert_eq!(last.label(), "BRIDGE");
        assert_eq!(phrase(99, 0, 1).label(), "SECTION 99");
        for invalid in [
            vec![phrase(1, 10, 5)],
            vec![phrase(1, 0, 20), phrase(5, 10, 30)],
            vec![phrase(5, 20, 30), phrase(1, 0, 10)],
        ] {
            assert!(Sections::new(invalid).is_err());
        }
        for range in [10.0..10.0, 10.0..5.0, f64::NAN..10.0, 0.0..f64::INFINITY] {
            assert!(sections.visible(range).next().is_none());
        }
    }

    #[test]
    fn phrase_geometry_clips_at_zoom_edges_and_keeps_leading_silence_and_gaps() {
        let sections = Sections::new(vec![
            phrase(1, 5_000, 12_125),
            phrase(5, 12_125, 14_000),
            phrase(3, 18_000, 30_000),
        ])
        .unwrap();
        let ctx = Context::default();
        let rect = Rect::from_min_size(pos2(20.0, 20.0), vec2(600.0, STRIP_HEIGHT));
        for (range, expected) in [
            (
                0.0..40.0,
                vec![(95.0, 201.875), (201.875, 230.0), (290.0, 470.0)],
            ),
            (
                10.0..20.0,
                vec![(20.0, 147.5), (147.5, 260.0), (500.0, 620.0)],
            ),
        ] {
            let mut output = ctx.run_ui(RawInput::default(), |ui| {
                sections.paint(ui.painter(), rect, range.clone())
            });
            output.textures_delta.clear();
            let bars: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Rect(r) if sections.0.iter().any(|p| p.color() == r.fill) => {
                        Some(r.rect)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(bars.len(), expected.len());
            for (bar, (left, right)) in bars.iter().zip(expected) {
                assert!(rect.contains_rect(*bar));
                assert!((bar.left() - left).abs() < 0.001 && (bar.right() - right).abs() < 0.001);
            }
            for shape in &output.shapes {
                if let Shape::Text(text) = &shape.shape {
                    let label = Rect::from_min_size(text.pos, text.galley.size());
                    assert!(bars.iter().any(|bar| bar.contains_rect(label)));
                }
            }
        }
    }

    #[test]
    fn narrow_sections_keep_their_color_without_overlapping_labels() {
        let sections = Sections::new(vec![phrase(1, 0, 1000), phrase(5, 1000, 100_000)]).unwrap();
        let ctx = Context::default();
        let mut output = ctx.run_ui(RawInput::default(), |ui| {
            sections.paint(
                ui.painter(),
                Rect::from_min_size(Pos2::ZERO, vec2(300.0, STRIP_HEIGHT)),
                0.0..100.0,
            )
        });
        output.textures_delta.clear();
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Text(t) => Some(t.galley.text()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, ["CHORUS"]);
        assert!(output.shapes.iter().any(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == sections.0[0].color() && r.rect.width() == 3.0)));
    }

    #[test]
    fn labels_take_their_colour_and_a_contrasting_text_colour() {
        let named = |name: &str| Phrase {
            name: name.into(),
            ..phrase(0, 0, 1)
        };
        assert_eq!(
            named("VERSE 3").color(),
            Color32::from_rgb(0x64, 0x50, 0xff)
        );
        assert_eq!(
            named("chorus 2").color(),
            Color32::from_rgb(0x00, 0xaa, 0x7d)
        );
        assert_eq!(phrase(2, 0, 1).color(), named("UP 1").color());
        assert_eq!(named("SECTION 4").color(), Color32::from_rgb(147, 157, 168));
        assert_eq!(named("BRIDGE").text_color(), Color32::from_black_alpha(204));
        assert_eq!(named("INTRO 1").text_color(), Color32::WHITE);
    }
}
