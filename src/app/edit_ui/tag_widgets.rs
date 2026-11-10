// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

pub(super) const TILE_HEIGHT: f32 = 56.0;
pub(super) const GAP: f32 = 8.0;

/// Include unknown IDs so malformed catalogs cannot hide tags from the browser.
pub(super) fn category_ids(catalog: &Catalog) -> Vec<u32> {
    let mut ids: Vec<_> = catalog
        .categories
        .iter()
        .map(|c| c.id)
        .chain(catalog.tags.iter().map(|tag| tag.category_id))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    ids.sort_by_key(|id| (catalog.category(*id).to_lowercase(), *id));
    ids
}

impl App {
    pub(super) fn tag_search(
        &mut self,
        ui: &mut Ui,
        query: &mut String,
        typing: &mut bool,
        hint: &str,
    ) {
        ui.horizontal(|ui| {
            let width = ui.available_width() - if query.is_empty() { 0.0 } else { 138.0 };
            let search = ui.add_sized(
                vec2(width, 48.0),
                Button::new(if query.is_empty() {
                    hint.into()
                } else {
                    format!("Search: {}", query)
                })
                .truncate()
                .fill(CARD)
                .corner_radius(8),
            );
            self.hit(&format!("Edit {hint}"), search.rect);
            if search.clicked() {
                *typing = true;
            }
            if !query.is_empty() && self.edit_button(ui, "Clear search", 132.0, false) {
                query.clear();
            }
        });
    }
}

pub(super) fn category_row(ui: &mut Ui, title: &str, count: usize, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::click());
    let p = ui.painter();
    p.rect_filled(
        rect,
        6,
        if selected {
            SELECTED
        } else if response.hovered() {
            CARD
        } else {
            PANEL
        },
    );
    if selected {
        p.rect_filled(rect.with_max_x(rect.left() + 3.0), 1, ORANGE);
    }
    theme::text(
        p,
        rect.shrink2(vec2(10.0, 4.0))
            .with_max_x(rect.right() - 38.0),
        title,
        15.0,
        if selected { ORANGE } else { TEXT },
    );
    p.text(
        pos2(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        count.to_string(),
        FontId::proportional(12.0),
        MUTED,
    );
    response.on_hover_text(title)
}

/// `checked` draws an assignment checkbox; `None` opens an editor instead.
pub(super) fn tile(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    width: f32,
    checked: Option<bool>,
) -> Response {
    let selected = checked == Some(true);
    let (rect, response) = ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 8, if selected { SELECTED } else { CARD });
    p.rect_stroke(
        rect,
        8,
        Stroke::new(
            1.0,
            if selected || response.hovered() {
                ORANGE
            } else {
                LINE
            },
        ),
        StrokeKind::Inside,
    );
    let check =
        Rect::from_center_size(pos2(rect.right() - 22.0, rect.center().y), vec2(22.0, 22.0));
    if checked.is_some() {
        p.rect_filled(check, 4, if selected { ORANGE } else { PANEL });
    }
    if selected {
        p.line_segment(
            [
                check.left_center() + vec2(5.0, 0.0),
                check.center() + vec2(-1.0, 5.0),
            ],
            Stroke::new(2.0, BG),
        );
        p.line_segment(
            [
                check.center() + vec2(-1.0, 5.0),
                check.right_top() + vec2(-4.0, 6.0),
            ],
            Stroke::new(2.0, BG),
        );
    } else if checked.is_some() {
        p.rect_stroke(check, 4, Stroke::new(1.0, LINE), StrokeKind::Inside);
    } else {
        let center = check.center();
        p.add(Shape::line(
            vec![
                center + vec2(-3.0, -6.0),
                center + vec2(3.0, 0.0),
                center + vec2(-3.0, 6.0),
            ],
            Stroke::new(1.5, MUTED),
        ));
    }
    theme::text(
        p,
        Rect::from_min_max(
            rect.min + vec2(12.0, 5.0),
            pos2(check.left() - 8.0, rect.top() + 30.0),
        ),
        title,
        16.0,
        TEXT,
    );
    theme::text(
        p,
        Rect::from_min_max(
            rect.min + vec2(12.0, 31.0),
            pos2(check.left() - 8.0, rect.bottom() - 5.0),
        ),
        subtitle,
        11.0,
        MUTED,
    );
    response.on_hover_text(format!("{title} · {subtitle}"))
}
