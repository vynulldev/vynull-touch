// SPDX-License-Identifier: GPL-3.0-or-later
use eframe::egui::{self, *};

pub const BG: Color32 = Color32::from_rgb(16, 19, 23);
pub const PANEL: Color32 = Color32::from_rgb(23, 27, 32);
pub const CARD: Color32 = Color32::from_rgb(30, 35, 41);
pub const LINE: Color32 = Color32::from_rgb(43, 49, 56);
pub const TEXT: Color32 = Color32::from_rgb(235, 238, 241);
pub const MUTED: Color32 = Color32::from_rgb(147, 157, 168);
pub const ORANGE: Color32 = Color32::from_rgb(255, 139, 65);
pub const SELECTED: Color32 = Color32::from_rgb(59, 39, 29);
pub const GREEN: Color32 = Color32::from_rgb(118, 216, 164);

pub fn configure(ctx: &Context, scale: f32) {
    ctx.set_zoom_factor(scale);
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "NotoSans-Bold".into(),
        FontData::from_static(include_bytes!("fonts/NotoSans-Bold.ttf")).into(),
    );
    let mut bold = fonts.families[&FontFamily::Proportional].clone();
    bold.insert(0, "NotoSans-Bold".into());
    fonts.families.insert(FontFamily::Name("Bold".into()), bold);
    ctx.set_fonts(fonts);
    let mut style = Style {
        visuals: Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = BG;
    style.visuals.window_fill = PANEL;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.extreme_bg_color = BG;
    style.visuals.selection.bg_fill = SELECTED;
    style.visuals.selection.stroke = Stroke::new(1.0, ORANGE);
    style.visuals.widgets.inactive.weak_bg_fill = CARD;
    style.visuals.widgets.inactive.bg_fill = CARD;
    style.visuals.widgets.inactive.bg_stroke = Stroke::NONE;
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 54, 61);
    style.visuals.widgets.active.weak_bg_fill = SELECTED;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, ORANGE);
    style.spacing.item_spacing = vec2(8.0, 8.0);
    style.spacing.button_padding = vec2(14.0, 10.0);
    style.spacing.interact_size = vec2(48.0, 48.0);
    style.spacing.scroll.bar_width = 6.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(17.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(16.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(25.0));
    ctx.set_theme(Theme::Dark);
    ctx.set_style_of(Theme::Dark, style);
}

pub fn button(ui: &mut Ui, label: &str, size: Vec2, active: bool) -> Response {
    ui.add_sized(
        size,
        Button::new(RichText::new(label).color(if active { ORANGE } else { TEXT }))
            .fill(if active { SELECTED } else { CARD })
            .corner_radius(8),
    )
}

pub fn primary(ui: &mut Ui, label: &str, size: Vec2) -> Response {
    ui.add_sized(
        size,
        Button::new(RichText::new(label).color(BG).strong())
            .fill(ORANGE)
            .corner_radius(8),
    )
}

pub fn text(painter: &Painter, rect: Rect, text: impl Into<String>, size: f32, color: Color32) {
    text_with_font(painter, rect, text, FontId::proportional(size), color);
}

pub fn bold_text(
    painter: &Painter,
    rect: Rect,
    text: impl Into<String>,
    size: f32,
    color: Color32,
) {
    text_with_font(
        painter,
        rect,
        text,
        FontId::new(size, FontFamily::Name("Bold".into())),
        color,
    );
}

fn text_with_font(
    painter: &Painter,
    rect: Rect,
    text: impl Into<String>,
    font: FontId,
    color: Color32,
) {
    let mut job = egui::text::LayoutJob::simple(text.into(), font, color, rect.width().max(1.0));
    job.wrap.max_rows = 1;
    let galley = painter.layout_job(job);
    painter
        .with_clip_rect(rect.intersect(painter.clip_rect()))
        .galley(
            pos2(rect.left(), rect.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
}

pub fn icon(p: &Painter, center: Pos2, kind: usize, color: Color32) {
    let s = Stroke::new(1.7, color);
    let r = Rect::from_center_size(center, vec2(24.0, 22.0));
    match kind {
        0 => {
            for dy in [-7.0, 0.0, 7.0] {
                p.line_segment([center + vec2(-7.0, dy), center + vec2(11.0, dy)], s);
                p.circle_filled(center + vec2(-12.0, dy), 1.5, color);
            }
        }
        1 => {
            p.rect_stroke(r, 3, s, StrokeKind::Inside);
            p.line_segment(
                [
                    r.left_top() + vec2(2.0, 6.0),
                    r.right_top() + vec2(-2.0, 6.0),
                ],
                s,
            );
        }
        2 => {
            p.circle_stroke(center, 12.0, s);
            p.circle_stroke(center, 4.0, s);
            p.line_segment([center + vec2(2.0, -8.0), center + vec2(8.0, -2.0)], s);
        }
        3 => {
            let outline = (0..32)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::TAU / 32.0;
                    let radius = if matches!(i % 4, 1 | 2) { 11.0 } else { 8.5 };
                    center + vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            p.add(Shape::closed_line(outline, s));
            p.circle_stroke(center, 3.5, s);
        }
        5 => {
            p.add(Shape::closed_line(
                vec![
                    center + vec2(-8.0, -11.0),
                    center + vec2(8.0, -11.0),
                    center + vec2(8.0, 11.0),
                    center + vec2(0.0, 5.0),
                    center + vec2(-8.0, 11.0),
                ],
                s,
            ));
        }
        _ => {
            p.rect_stroke(r, 3, s, StrokeKind::Inside);
            p.line_segment([center + vec2(-7.0, -4.0), center + vec2(-3.0, 0.0)], s);
            p.line_segment([center + vec2(-3.0, 0.0), center + vec2(-7.0, 4.0)], s);
            p.line_segment([center + vec2(0.0, 4.0), center + vec2(7.0, 4.0)], s);
        }
    }
}

pub fn playlist_icon(p: &Painter, center: Pos2, folder: bool, smart: bool, color: Color32) {
    if folder {
        let outline = [
            vec2(-13.0, -10.0),
            vec2(-4.0, -10.0),
            vec2(0.0, -6.0),
            vec2(13.0, -6.0),
            vec2(13.0, 10.0),
            vec2(-13.0, 10.0),
        ]
        .into_iter()
        .map(|offset| center + offset)
        .collect();
        p.add(Shape::closed_line(outline, Stroke::new(1.7, color)));
    } else {
        icon(p, center, 1, color);
        if smart {
            let center = center + vec2(0.0, 3.0);
            let sparkle = (0..8)
                .map(|i| {
                    let angle = i as f32 * std::f32::consts::FRAC_PI_4;
                    let radius = if i % 2 == 0 { 7.0 } else { 2.3 };
                    center + vec2(angle.cos(), angle.sin()) * radius
                })
                .collect();
            p.add(Shape::closed_line(sparkle, Stroke::new(1.5, color)));
        }
    }
}

pub fn sleeve(p: &Painter, rect: Rect, id: u32) {
    let colors = [
        Color32::from_rgb(82, 91, 108),
        Color32::from_rgb(90, 71, 74),
        Color32::from_rgb(65, 92, 90),
        Color32::from_rgb(84, 76, 108),
    ];
    let c = colors[id as usize % 4];
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    p.rect_filled(rect, 5, c.gamma_multiply(0.4));
    let center = rect.center() + vec2(rect.width() * 0.12, 0.0);
    for i in 0..4 {
        p.circle_stroke(
            center,
            rect.width() * (0.17 + i as f32 * 0.12),
            Stroke::new(1.0, c),
        );
    }
    p.line_segment([rect.left_bottom(), rect.right_top()], Stroke::new(2.0, c));
}
