// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::diagnostics::Logs;

mod overview;

const ROW_HEIGHT: f32 = 22.0;

struct Row {
    timestamp: String,
    text: String,
    color: Color32,
    entry: usize,
}

#[derive(Default)]
pub(super) struct Diagnostics {
    pub logs: Logs,
    pub version: crate::diagnostics::version::Version,
    pub requests: overview::Requests,
    showing_logs: bool,
    rows: Vec<Row>,
    revision: u64,
    width: f32,
    #[cfg(test)]
    scroll_offset: f32,
}

impl App {
    pub(super) fn diagnostics(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let back = theme::button(ui, "‹ Settings", vec2(112.0, 48.0), false);
            self.hit("Diag back", back.rect);
            if back.clicked() {
                self.page = Page::Settings;
            }
            for (logs, label, width) in [(false, "Status", 96.0), (true, "Logs", 80.0)] {
                let tab = theme::button(
                    ui,
                    label,
                    vec2(width, 48.0),
                    self.diagnostics.showing_logs == logs,
                );
                self.hit(&format!("Diag tab {label}"), tab.rect);
                if tab.clicked() {
                    self.diagnostics.showing_logs = logs;
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if !self.diagnostics.showing_logs {
                    let copy = theme::button(ui, "Copy diagnostics", vec2(176.0, 48.0), false)
                        .on_hover_text("Copy status, versions, errors and any fetched logs");
                    self.hit("Copy diagnostics", copy.rect);
                    if copy.clicked() {
                        ui.ctx().copy_text(self.diagnostic_report());
                        self.notice = "Copied diagnostic status and cached server logs".into();
                    }
                    return;
                }
                let copy = ui
                    .add_enabled_ui(!self.diagnostics.logs.entries.is_empty(), |ui| {
                        theme::button(ui, "Copy logs", vec2(96.0, 48.0), false)
                    })
                    .inner;
                self.hit("Diag copy", copy.rect);
                if copy.clicked() {
                    ui.ctx().copy_text(self.diagnostics.logs.copy_text());
                    self.notice = format!(
                        "Copied {} server log entries",
                        self.diagnostics.logs.entries.len()
                    );
                }
                let latest = theme::button(ui, "Latest", vec2(80.0, 48.0), false);
                self.hit("Diag latest", latest.rect);
                if latest.clicked() {
                    self.diagnostics.logs.resume();
                }
                let pause = theme::button(
                    ui,
                    if self.diagnostics.logs.paused {
                        "Resume"
                    } else {
                        "Pause"
                    },
                    vec2(80.0, 48.0),
                    self.diagnostics.logs.paused,
                );
                self.hit("Diag pause", pause.rect);
                if pause.clicked() {
                    if self.diagnostics.logs.paused {
                        self.diagnostics.logs.resume();
                    } else {
                        self.diagnostics.logs.pause();
                    }
                }
            });
        });
        self.diagnostics.version.update(
            ui.ctx(),
            &self.options.server,
            self.options.demo,
            self.page == Page::Diag && !self.diagnostics.showing_logs,
        );
        if !self.diagnostics.showing_logs {
            self.diagnostics
                .logs
                .update(ui.ctx(), &self.options.server, self.options.demo, false);
            self.diagnostic_overview(ui);
            return;
        }
        let (origin, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        theme::text(
            ui.painter(),
            origin,
            if self.options.demo {
                "Demo server · fictional logs"
            } else {
                &self.options.server
            },
            12.0,
            MUTED,
        );

        let error_height = if self.diagnostics.logs.error.is_empty() {
            0.0
        } else {
            42.0
        };
        let height = (ui.available_height() - 30.0 - error_height).max(80.0);
        let viewport = Rect::from_min_size(
            ui.available_rect_before_wrap().min + vec2(0.0, error_height),
            vec2(ui.available_width(), height),
        );
        // Freeze before consuming a pending response, so touching the history
        // cannot replace the lines or shift their positions under the finger.
        if ui.input(|i| {
            i.pointer
                .interact_pos()
                .is_some_and(|p| viewport.contains(p))
                && (i.pointer.any_pressed() || i.smooth_scroll_delta() != Vec2::ZERO)
        }) {
            self.diagnostics.logs.pause();
        }
        self.diagnostics
            .logs
            .update(ui.ctx(), &self.options.server, self.options.demo, true);

        if !self.diagnostics.logs.error.is_empty() {
            let (error, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
            theme::text(
                ui.painter(),
                error,
                &self.diagnostics.logs.error,
                13.0,
                ORANGE,
            );
        }
        let height = (ui.available_height() - 30.0).max(80.0);
        let width = (ui.available_width() - 100.0).max(80.0);
        if self.diagnostics.revision != self.diagnostics.logs.revision
            || self.diagnostics.width != width
        {
            self.diagnostics.rows.clear();
            for (index, entry) in self.diagnostics.logs.entries.iter().enumerate() {
                let color = log_color(&entry.line);
                let galley =
                    ui.painter()
                        .layout(entry.line.clone(), FontId::monospace(13.0), color, width);
                for (line, row) in galley.rows.iter().enumerate() {
                    self.diagnostics.rows.push(Row {
                        timestamp: if line == 0 {
                            entry.timestamp()
                        } else {
                            String::new()
                        },
                        text: row.text(),
                        color,
                        entry: index,
                    });
                }
            }
            self.diagnostics.width = width;
            self.diagnostics.revision = self.diagnostics.logs.revision;
            ui.ctx().request_repaint();
        }
        let id = ui.make_persistent_id(("diagnostic-log", self.token));
        let count = self.diagnostics.rows.len();
        if !self.diagnostics.logs.paused {
            track_scroll::set_offset(ui.ctx(), id, (count as f32 * ROW_HEIGHT - height).max(0.0));
        }
        let spacing = ui.spacing().item_spacing.y;
        let rubber_band = ui.spacing().scroll.kinetic.rubber_band;
        ui.spacing_mut().item_spacing.y = 0.0;
        // Tail jumps can exceed the scroll area's previous content bounds.
        // Do not let its overscroll spring turn that jump into residual motion
        // when the user pauses to read; our touch gesture owns the fling.
        ui.spacing_mut().scroll.kinetic.rubber_band = false;
        let output = track_scroll::show_rows(ui, id, height, ROW_HEIGHT, count, |ui, range| {
            for row in range {
                let row = &self.diagnostics.rows[row];
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                let p = ui.painter();
                p.rect_filled(
                    rect,
                    0,
                    if row.entry.is_multiple_of(2) {
                        PANEL
                    } else {
                        BG
                    },
                );
                p.text(
                    rect.left_center() + vec2(5.0, 0.0),
                    Align2::LEFT_CENTER,
                    &row.timestamp,
                    FontId::monospace(12.0),
                    MUTED,
                );
                p.with_clip_rect(rect.intersect(ui.clip_rect())).text(
                    rect.left_center() + vec2(80.0, 0.0),
                    Align2::LEFT_CENTER,
                    &row.text,
                    FontId::monospace(13.0),
                    row.color,
                );
            }
        });
        ui.spacing_mut().item_spacing.y = spacing;
        ui.spacing_mut().scroll.kinetic.rubber_band = rubber_band;
        #[cfg(test)]
        {
            self.diagnostics.scroll_offset = output.state.clamped_offset().y;
        }
        self.hit("Diag logs", output.inner_rect);
        if count == 0 {
            let message = if !self.diagnostics.logs.error.is_empty() {
                "Server logs unavailable"
            } else if self.diagnostics.logs.updated.is_some() {
                "No server log entries yet"
            } else if self.diagnostics.logs.paused {
                "Paused · tap Resume to fetch logs"
            } else {
                "Fetching server logs…"
            };
            theme::text(
                ui.painter(),
                output.inner_rect.shrink(12.0),
                message,
                17.0,
                MUTED,
            );
        }
        let (status, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let state = if self.diagnostics.logs.paused {
            "PAUSED"
        } else if !self.diagnostics.logs.error.is_empty() {
            "RETRYING"
        } else {
            "LIVE"
        };
        let updated = self.diagnostics.logs.updated.map_or(String::new(), |at| {
            format!(" · updated {}s ago", at.elapsed().as_secs())
        });
        theme::text(
            ui.painter(),
            status,
            format!(
                "{state} · {} / 500 entries · UTC{updated} · touch to pause",
                self.diagnostics.logs.entries.len()
            ),
            12.0,
            MUTED,
        );
        self.hit("Diag status", status);
    }
}

fn log_color(line: &str) -> Color32 {
    let lower = line.to_lowercase();
    let mut warning = false;
    for word in lower.split(|c: char| !c.is_alphanumeric()) {
        if matches!(
            word,
            "error" | "err" | "failed" | "failure" | "fatal" | "panic"
        ) {
            return Color32::from_rgb(255, 123, 123);
        }
        warning |= matches!(word, "warn" | "warning");
    }
    if warning { ORANGE } else { TEXT }
}

#[cfg(test)]
mod tests;
