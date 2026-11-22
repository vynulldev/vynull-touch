// SPDX-License-Identifier: GPL-3.0-or-later
use eframe::egui::{self, *};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    Classic,
    #[default]
    #[serde(other)]
    XdjAz,
}

impl UiTheme {
    pub const ALL: [Self; 2] = [Self::Classic, Self::XdjAz];

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::XdjAz => "XDJ-AZ",
        }
    }

    pub fn is_xdj(self) -> bool {
        self == Self::XdjAz
    }

    pub fn palette(self) -> Palette {
        match self {
            Self::Classic => Palette {
                bg: Color32::from_rgb(16, 19, 23),
                panel: Color32::from_rgb(23, 27, 32),
                card: Color32::from_rgb(30, 35, 41),
                line: Color32::from_rgb(43, 49, 56),
                text: Color32::from_rgb(235, 238, 241),
                muted: Color32::from_rgb(147, 157, 168),
                accent: ORANGE,
                accent_text: ORANGE,
                row_selected: Color32::from_rgb(91, 49, 26),
                selected: Color32::from_rgb(59, 39, 29),
                radius: 8,
            },
            Self::XdjAz => Palette {
                bg: BG,
                panel: PANEL,
                card: CARD,
                line: LINE,
                text: TEXT,
                muted: MUTED,
                accent: BLUE,
                accent_text: CYAN,
                row_selected: ROW_SELECTED,
                selected: SELECTED,
                radius: 2,
            },
        }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    pub bg: Color32,
    pub panel: Color32,
    pub card: Color32,
    pub line: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub accent: Color32,
    pub accent_text: Color32,
    pub row_selected: Color32,
    pub selected: Color32,
    pub radius: u8,
}

// Keep the choice in the egui context so shared painters use the same palette
// as widgets, without process-wide state or changing waveform/cue colors.
pub fn current(ctx: &Context) -> UiTheme {
    ctx.data(|data| data.get_temp(Id::unique("ui-theme")).unwrap_or_default())
}

pub fn palette(ctx: &Context) -> Palette {
    current(ctx).palette()
}

pub const BG: Color32 = Color32::from_rgb(4, 5, 7);
const PANEL: Color32 = Color32::from_rgb(24, 25, 28);
const CARD: Color32 = Color32::from_rgb(39, 41, 45);
pub const LINE: Color32 = Color32::from_rgb(64, 67, 73);
pub const TEXT: Color32 = Color32::from_rgb(245, 246, 248);
const MUTED: Color32 = Color32::from_rgb(166, 171, 180);
const BLUE: Color32 = Color32::from_rgb(0, 139, 232);
const CYAN: Color32 = Color32::from_rgb(57, 178, 255);
const ROW_SELECTED: Color32 = Color32::from_rgb(76, 78, 91);
pub const ON_AIR: Color32 = Color32::from_rgb(212, 24, 35);
pub const ORANGE: Color32 = Color32::from_rgb(255, 139, 65);
const SELECTED: Color32 = Color32::from_rgb(15, 48, 76);
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
    apply(ctx, UiTheme::default());
}

pub fn apply(ctx: &Context, theme: UiTheme) {
    ctx.data_mut(|data| data.insert_temp(Id::unique("ui-theme"), theme));
    let colors = theme.palette();
    let mut style = Style {
        visuals: Visuals::dark(),
        ..Default::default()
    };
    style.visuals.panel_fill = colors.bg;
    style.visuals.window_fill = colors.panel;
    style.visuals.override_text_color = Some(colors.text);
    style.visuals.extreme_bg_color = colors.bg;
    style.visuals.selection.bg_fill = colors.selected;
    style.visuals.selection.stroke = Stroke::new(1.0, colors.accent_text);
    style.visuals.widgets.inactive.weak_bg_fill = colors.card;
    style.visuals.widgets.inactive.bg_fill = colors.card;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, colors.line);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(49, 54, 61);
    style.visuals.widgets.active.weak_bg_fill = colors.selected;
    style.visuals.widgets.active.fg_stroke = Stroke::new(1.5, colors.accent_text);
    style.visuals.widgets.inactive.corner_radius = CornerRadius::same(colors.radius);
    style.visuals.widgets.hovered.corner_radius = CornerRadius::same(colors.radius);
    style.visuals.widgets.active.corner_radius = CornerRadius::same(colors.radius);
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
    ctx.request_repaint();
}

pub fn button(ui: &mut Ui, label: &str, size: Vec2, active: bool) -> Response {
    let colors = palette(ui.ctx());
    ui.add_sized(
        size,
        Button::new(RichText::new(label).color(if active {
            colors.accent_text
        } else {
            colors.text
        }))
        .fill(if active { colors.selected } else { colors.card })
        .stroke(if current(ui.ctx()).is_xdj() {
            Stroke::new(1.0, if active { colors.accent } else { colors.line })
        } else {
            Stroke::NONE
        })
        .corner_radius(colors.radius),
    )
}

pub fn primary(ui: &mut Ui, label: &str, size: Vec2) -> Response {
    let colors = palette(ui.ctx());
    ui.add_sized(
        size,
        Button::new(
            RichText::new(label)
                .color(if current(ui.ctx()).is_xdj() {
                    colors.text
                } else {
                    colors.bg
                })
                .strong(),
        )
        .fill(colors.accent)
        .corner_radius(colors.radius),
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
        6 => {
            p.circle_stroke(center, 12.0, s);
            p.circle_filled(center, 7.0, color);
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
