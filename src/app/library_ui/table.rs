// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::waveform::{self, Overview, Style};

const ROW_HEIGHT: f32 = 50.0;
const HEADER_HEIGHT: f32 = 48.0;

/// One geometry for both the fixed header and virtualized rows. Reserve the
/// scrollbar gutter even when the list is short, so columns never shift.
struct Columns {
    widths: [f32; 6],
}

impl Columns {
    fn new(width: f32) -> Self {
        let preview = (width * 0.21).clamp(128.0, 240.0);
        let bpm = 76.0;
        let key = 56.0;
        let duration = 60.0;
        let names = (width - preview - bpm - key - duration).max(100.0);
        Self {
            widths: [preview, names * 0.57, names * 0.43, bpm, key, duration],
        }
    }

    fn rects(&self, row: Rect) -> [Rect; 6] {
        let mut x = row.left();
        self.widths.map(|width| {
            let cell = Rect::from_min_size(pos2(x, row.top()), vec2(width, row.height()));
            x += width;
            cell
        })
    }
}

impl App {
    pub(in crate::app) fn library_table(&mut self, ui: &mut Ui, height: f32) -> Option<Track> {
        let mut inspect = None;
        let searching = self.keyboard == Some(Field::Search);
        // Keep at least two full touch rows above the keyboard at 800×480.
        // Larger screens retain the sortable header while searching.
        let header_height = if searching && height < HEADER_HEIGHT + 3.0 * ROW_HEIGHT {
            0.0
        } else {
            HEADER_HEIGHT
        };
        ui.scope(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            let width = (ui.available_width() - 12.0).max(500.0);
            let columns = Columns::new(width);
            if header_height > 0.0 {
                let (header, _) =
                    ui.allocate_exact_size(vec2(width, header_height), Sense::hover());
                ui.painter().rect_filled(header, 0, CARD);
                for (cell, (label, field)) in columns.rects(header).into_iter().zip([
                    ("Preview", None),
                    ("Track", Some(Sort::Title)),
                    ("Artist", Some(Sort::Artist)),
                    ("BPM", Some(Sort::Bpm)),
                    ("Key", Some(Sort::Key)),
                    ("Time", Some(Sort::Duration)),
                ]) {
                    let active = field == Some(self.order.field);
                    let color = if active { ORANGE } else { MUTED };
                    let mut text = cell.shrink2(vec2(8.0, 0.0));
                    if active {
                        text.max.x -= 12.0;
                        let center = cell.right_center() - vec2(10.0, 0.0);
                        let direction = if self.order.descending { 1.0 } else { -1.0 };
                        ui.painter().add(Shape::convex_polygon(
                            vec![
                                center + vec2(-4.0, -3.0 * direction),
                                center + vec2(4.0, -3.0 * direction),
                                center + vec2(0.0, 3.0 * direction),
                            ],
                            color,
                            Stroke::NONE,
                        ));
                    }
                    theme::text(ui.painter(), text, label, 13.0, color);
                    ui.painter()
                        .vline(cell.right(), cell.y_range(), Stroke::new(1.0, BG));
                    if let Some(field) = field {
                        let response = ui.interact(
                            cell,
                            ui.scope_id().with(("library-column", field)),
                            Sense::click(),
                        );
                        self.hit(&format!("Column {}", field.label()), cell);
                        if response.clicked() {
                            self.apply_order(if active && self.order.descending {
                                Order::default()
                            } else {
                                Order {
                                    field,
                                    descending: active,
                                }
                            });
                        }
                    }
                }
            }
            let drag_id = ui.make_persistent_id((
                "track-drag",
                self.source,
                &self.query,
                self.order,
                self.filter_revision,
            ));
            self.library_scroll = Some(drag_id);
            let rows = track_scroll::show_rows(
                ui,
                drag_id,
                (height - header_height).max(20.0),
                ROW_HEIGHT,
                self.visible.len(),
                |ui, range| {
                    for row in range {
                        let track = self.tracks[self.visible[row]].clone();
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::click());
                        let selected = self.selected == Some(track.id);
                        ui.painter().rect_filled(
                            rect,
                            0,
                            if selected {
                                Color32::from_rgb(91, 49, 26)
                            } else if row % 2 == 0 {
                                PANEL
                            } else {
                                BG
                            },
                        );
                        if selected {
                            ui.painter().rect_filled(
                                Rect::from_min_size(rect.min, vec2(3.0, rect.height())),
                                0,
                                ORANGE,
                            );
                        }
                        let cells = columns.rects(rect);
                        let cover = Rect::from_min_size(
                            pos2(cells[0].right() - 46.0, rect.top() + 4.0),
                            vec2(42.0, 42.0),
                        );
                        let wave = Rect::from_min_max(
                            cells[0].min + vec2(6.0, 4.0),
                            pos2(cover.left() - 4.0, rect.bottom() - 4.0),
                        );
                        let visible = ui.is_rect_visible(rect);
                        let art = if visible {
                            self.artwork
                                .as_mut()
                                .and_then(|cache| cache.texture(&track))
                        } else {
                            None
                        };
                        if let Some(texture) = art {
                            Image::new((texture, cover.size())).paint_at(ui, cover);
                        } else {
                            theme::sleeve(ui.painter(), cover, track.id);
                        }
                        ui.painter().rect_filled(wave, 0, Color32::BLACK);
                        let texture = if visible {
                            self.artwork.as_mut().and_then(|cache| {
                                cache.waveform(&track, self.waveform_style, self.waveform_overview)
                            })
                        } else {
                            None
                        };
                        if let Some(texture) = texture {
                            Image::new((texture, wave.size())).paint_at(ui, wave);
                        } else if self.options.demo && !track.file_missing {
                            demo_waveform(
                                ui.painter(),
                                wave,
                                track.id,
                                self.waveform_style,
                                self.waveform_overview,
                            );
                        } else {
                            ui.painter().hline(
                                wave.x_range(),
                                wave.bottom() - 1.0,
                                Stroke::new(1.0, LINE),
                            );
                            theme::text(ui.painter(), wave.shrink(4.0), "—", 12.0, MUTED);
                        }
                        let cues = if self.options.demo {
                            self.demo_cues.get(&track.id).map_or(&[][..], Vec::as_slice)
                        } else {
                            self.refresh.cues(track.id)
                        };
                        waveform::markers::hot_cues(
                            ui.painter(),
                            wave,
                            0.0..f64::from(track.duration),
                            cues,
                            waveform::markers::Edge::Top,
                        );
                        for (i, value) in [
                            track.title.clone(),
                            track.artist.clone(),
                            if track.bpm.is_finite() && track.bpm > 0.0 {
                                format!("{:.1}", track.bpm)
                            } else {
                                "—".into()
                            },
                            if track.key.is_empty() {
                                "—".into()
                            } else {
                                track.key.clone()
                            },
                            if track.duration > 0 {
                                time(track.duration.into())
                            } else {
                                "—".into()
                            },
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            let cell = cells[i + 1].shrink2(vec2(8.0, 0.0));
                            let size = if i == 0 {
                                17.0
                            } else if i == 4 {
                                13.0
                            } else {
                                15.0
                            };
                            let color = if track.file_missing {
                                MUTED
                            } else if i == 3 {
                                GREEN
                            } else if i == 4 {
                                MUTED
                            } else {
                                TEXT
                            };
                            if i == 0 && track.file_missing {
                                theme::text(
                                    ui.painter(),
                                    Rect::from_min_max(
                                        cell.min,
                                        pos2(cell.right(), cell.center().y + 3.0),
                                    ),
                                    value,
                                    size,
                                    color,
                                );
                                theme::text(
                                    ui.painter(),
                                    Rect::from_min_max(
                                        pos2(cell.left(), cell.center().y + 3.0),
                                        cell.max,
                                    ),
                                    "File missing",
                                    10.0,
                                    ORANGE,
                                );
                            } else {
                                theme::text(ui.painter(), cell, value, size, color);
                            }
                        }
                        for cell in &cells[..5] {
                            ui.painter().vline(
                                cell.right(),
                                cell.y_range(),
                                Stroke::new(1.0, LINE),
                            );
                        }
                        ui.painter()
                            .hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, LINE));
                        self.hit(&format!("Track {}", track.id), rect);
                        self.hit(&format!("Preview {}", track.id), wave);
                        self.hit(&format!("Artwork {}", track.id), cover);
                        if response.clicked() {
                            self.selected = Some(track.id);
                            if searching {
                                self.keyboard = None;
                            }
                        }
                        if !searching && response.double_clicked() {
                            inspect = Some(track);
                        }
                    }
                },
            );
            self.hit("Library results", rows.inner_rect);
            if self.visible.is_empty() {
                let message = if self.loading {
                    "Loading your collection…"
                } else if !self.connection_error.is_empty() {
                    "Connection unavailable. Retrying automatically…"
                } else if !self.query.is_empty() || self.filters.count() > 0 {
                    "No matching tracks. Adjust your search or filters."
                } else {
                    "No tracks here yet. Add music through Vynull."
                };
                theme::text(
                    ui.painter(),
                    rows.inner_rect.shrink(8.0),
                    message,
                    16.0,
                    MUTED,
                );
                self.hit("Library empty", rows.inner_rect);
            }
        });
        inspect
    }
}

fn demo_waveform(p: &Painter, rect: Rect, id: u32, style: Style, overview: Overview) {
    let bands = crate::demo::library_waveform(id);
    if style.uses_bands(true) {
        waveform::half_band_columns(p, rect, &bands);
    } else {
        for (i, band) in bands.iter().enumerate() {
            let cell = Rect::from_min_max(
                pos2(
                    rect.left() + i as f32 * rect.width() / bands.len() as f32,
                    rect.top(),
                ),
                pos2(
                    rect.left() + (i + 1) as f32 * rect.width() / bands.len() as f32,
                    rect.bottom(),
                ),
            );
            waveform::column(
                p,
                cell,
                style,
                overview,
                Some(WaveEntry {
                    r: band.mid / 32,
                    g: band.high / 32,
                    b: band.bass / 32,
                    h: ((u16::from(band.bass) + u16::from(band.mid) + u16::from(band.high)) / 20)
                        .min(31) as u8,
                }),
                None,
            );
        }
    }
}
