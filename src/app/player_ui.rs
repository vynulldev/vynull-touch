// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::decks::{Assets, Key, View};
use std::sync::Arc;

mod compare;
mod dock;
mod focus;
mod pair;
#[cfg(test)]
mod phrase_tests;
mod phrase_ui;
#[cfg(test)]
mod tests;
mod timing;

struct DeckSnapshot {
    peer: Option<Peer>,
    player: Option<Player>,
    local: Option<Track>,
    view: View,
    assets: Assets,
    position: Option<f64>,
    waveform_position: Option<f64>,
    live: bool,
}

impl App {
    pub(super) fn players(&mut self, ui: &mut Ui) {
        if let Some(number) = self.deck_focus {
            self.player_focus(ui, number);
            return;
        }
        if self.deck_compare {
            self.player_compare(ui, self.deck_pair);
            return;
        }
        // Use two rows only when four full-size touch cards fit without scrolling.
        // The selected pair keeps its order in the first row as the window resizes.
        let rows = if ui.available_width() >= 680.0 && ui.available_height() >= 632.0 {
            2
        } else {
            1
        };
        let mut numbers = self.deck_pair.to_vec();
        if rows == 2 {
            numbers.extend((1..=4).filter(|n| !self.deck_pair.contains(n)).take(2));
        }
        // Keep the overview waveform at most 96 points tall in either layout.
        let height =
            ((ui.available_height() - 8.0 * (rows - 1) as f32) / rows as f32).clamp(312.0, 356.0);
        let width = (ui.available_width() - 8.0) / 2.0;
        ScrollArea::vertical()
            .id_salt("decks")
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut cards = Vec::with_capacity(numbers.len());
                for pair in numbers.chunks(2) {
                    ui.horizontal_top(|ui| {
                        for &number in pair {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(width, height), Sense::hover());
                            cards.push((number, rect));
                        }
                    });
                }
                for (number, rect) in cards {
                    self.deck_card(ui, rect, number);
                }
            });
        if self.fresh()
            && self.linked
            && self.status.players.iter().any(Player::is_forward_playback)
        {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Put view controls in the shared header so the performance screen can use
    /// its entire body for waveforms and large deck readouts.
    pub(super) fn players_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if self.deck_focus.is_none() {
                if self.deck_compare {
                    for (label, factor) in [("−", 2.0), ("+", 0.5)] {
                        let span = (self.deck_span * factor).clamp(4.0, 64.0);
                        let button = ui
                            .add_enabled_ui(span != self.deck_span, |ui| {
                                theme::button(ui, label, vec2(48.0, 48.0), false)
                            })
                            .inner;
                        self.hit(&format!("Compare zoom {label}"), button.rect);
                        if button.clicked() {
                            self.deck_span = span;
                            self.save_players();
                        }
                    }
                }
                let compare = theme::button(
                    ui,
                    if self.deck_compare {
                        "Cards"
                    } else {
                        "Waveforms"
                    },
                    vec2(96.0, 48.0),
                    false,
                );
                self.hit("Deck compare", compare.rect);
                if compare.clicked() {
                    self.deck_compare = !self.deck_compare;
                    self.save_players();
                }
                let pair = theme::button(
                    ui,
                    &format!("Pair {} + {}…", self.deck_pair[0], self.deck_pair[1]),
                    vec2(112.0, 48.0),
                    false,
                );
                self.hit("Deck pair", pair.rect);
                if pair.clicked() {
                    self.deck_picker = Some(self.deck_pair);
                }
            }
            let matches = theme::button(ui, "Match…", vec2(106.0, 48.0), false);
            self.hit("Match a player", matches.rect);
            if matches.clicked() {
                self.open_matches();
            }
        });
    }

    fn deck_snapshot(&mut self, number: u8) -> DeckSnapshot {
        let peer = self
            .status
            .peers
            .iter()
            .find(|p| p.device_number == number && p.device_type == "cdj")
            .cloned();
        let mut player = self
            .status
            .players
            .iter()
            .find(|p| p.device_number == number)
            .cloned();
        if self.options.demo
            && let Some(p) = &mut player
            && p.is_forward_playback()
        {
            let beats = self.demo_started.elapsed().as_secs_f64() * p.bpm / 60.0
                * (1.0 + p.pitch_pct / 100.0);
            p.beat_in_track += beats.floor() as u32;
            p.beat_in_bar =
                ((u32::from(p.beat_in_bar.saturating_sub(1)) + beats.floor() as u32) % 4 + 1) as u8;
            p.beat_age_ms =
                (beats.fract() * 60_000.0 / (p.bpm * (1.0 + p.pitch_pct / 100.0))) as u32;
        }
        let local = player
            .as_ref()
            .filter(|p| !p.external)
            .and_then(|p| self.all_tracks.iter().find(|t| t.id == p.track_id))
            .cloned();
        let key = player.as_ref().and_then(|p| Key::new(peer.as_ref(), p));
        let view = if self.options.demo {
            if let Some(track) = &local {
                let cues = self.demo_cues.get(&track.id).cloned();
                let data = self
                    .demo_decks
                    .entry(track.id)
                    .or_insert_with(|| {
                        let mut a = Assets::demo(track);
                        if let Some(grid) = self.demo_grids.get(&track.id) {
                            a.grid = Arc::new(grid.grid.clone());
                            a.beats = grid.grid.beats().iter().map(|b| b.seconds()).collect();
                            a.bpm = grid.bpm;
                        }
                        if let Some(cues) = cues {
                            a.cues = cues;
                        }
                        Arc::new(a)
                    })
                    .clone();
                View { data, art: None }
            } else {
                View::default()
            }
        } else {
            key.as_ref()
                .and_then(|key| self.deck_cache.as_mut().map(|cache| cache.get(key)))
                .unwrap_or_default()
        };
        let mut assets = (*view.data).clone();
        if let Some(track) = &local {
            if assets.duration <= 0.0 {
                assets.duration = player
                    .as_ref()
                    .filter(|p| p.duration_ms > 0)
                    .map_or(f64::from(track.duration), |p| {
                        f64::from(p.duration_ms) / 1000.0
                    });
            }
            if assets.bpm <= 0.0 {
                assets.bpm = track.bpm;
            }
        }
        // An external rekordbox ID may collide with a library ID. Its duration,
        // grid and artwork are resolved solely through its external source tuple.
        let live = self.fresh() && self.linked && peer.is_some();
        let now = Instant::now();
        let sampled = if self.options.demo {
            now
        } else {
            self.last_status.unwrap_or(now)
        };
        let position = player.as_ref().zip(key.as_ref()).and_then(|(p, key)| {
            self.deck_clocks
                .entry(key.clone())
                .or_default()
                .position(p, &assets, sampled, now, live)
        });
        // Loaded decks may omit their beat counter while CUED/PAUSED. Keep
        // drawing their analysis without inventing a current playback position.
        let waveform_position = key.as_ref().map(|key| {
            position
                .or_else(|| {
                    self.deck_clocks
                        .get(key)
                        .and_then(crate::decks::Clock::last_position)
                })
                .unwrap_or(0.0)
        });
        DeckSnapshot {
            peer,
            player,
            local,
            view,
            assets,
            position,
            waveform_position,
            live,
        }
    }

    fn deck_card(&mut self, ui: &mut Ui, rect: Rect, number: u8) {
        let DeckSnapshot {
            peer,
            player,
            local,
            view,
            assets,
            position,
            live,
            ..
        } = self.deck_snapshot(number);
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        painter.rect_filled(rect, 0, PANEL);
        let border = if live && player.as_ref().is_some_and(|p| p.on_air) {
            GREEN
        } else {
            LINE
        };
        painter.rect_stroke(rect, 0, Stroke::new(1.0, border), StrokeKind::Inside);
        let inside = rect.shrink(12.0);
        theme::text(
            &painter,
            Rect::from_min_size(inside.min, vec2(105.0, 22.0)),
            format!("PLAYER {number:02} ›"),
            14.0,
            ORANGE,
        );
        theme::text(
            &painter,
            Rect::from_min_max(
                inside.min + vec2(110.0, 0.0),
                pos2(inside.right(), inside.top() + 22.0),
            ),
            player.as_ref().filter(|p| !p.name.is_empty()).map_or_else(
                || peer.as_ref().map_or("Not discovered", |p| p.name.as_str()),
                |p| p.name.as_str(),
            ),
            11.0,
            MUTED,
        );
        let state = if !self.fresh() {
            "STALE"
        } else if !self.linked {
            "LINK OFF"
        } else if peer.is_none() {
            "DISCONNECTED"
        } else {
            player.as_ref().map_or("WAITING FOR STATUS", |p| {
                if p.play_state_name.is_empty() {
                    "UNKNOWN"
                } else {
                    &p.play_state_name
                }
            })
        };
        let flags = player
            .as_ref()
            .map(|p| {
                [
                    p.is_master.then_some("MASTER"),
                    p.is_sync.then_some("SYNC"),
                    p.on_air.then_some("ON AIR"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            })
            .unwrap_or_default();
        theme::text(
            &painter,
            Rect::from_min_size(inside.min + vec2(0.0, 25.0), vec2(120.0, 18.0)),
            state,
            11.0,
            if live { GREEN } else { MUTED },
        );
        painter.text(
            inside.right_top() + vec2(0.0, 34.0),
            Align2::RIGHT_CENTER,
            if live { flags.as_str() } else { "" },
            FontId::proportional(10.0),
            ORANGE,
        );
        let open = ui.interact(
            Rect::from_min_size(inside.min, vec2(inside.width(), 48.0)),
            ui.scope_id().with(("focus", number)),
            Sense::click(),
        );
        self.hit(&format!("Deck focus {number}"), open.rect);
        if open.clicked() {
            self.deck_focus = Some(number);
        }
        let art = Rect::from_min_size(inside.min + vec2(0.0, 50.0), vec2(60.0, 60.0));
        if let Some(texture) = &view.art {
            painter.image(
                texture.id(),
                art,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            theme::sleeve(
                &painter,
                art,
                player.as_ref().map_or(u32::from(number), |p| p.track_id),
            );
        }
        let metadata = |y: f32, h: f32| {
            Rect::from_min_max(
                inside.min + vec2(70.0, y),
                pos2(inside.right(), inside.top() + y + h),
            )
        };
        let title = player.as_ref().map_or("No player connected", |p| {
            if !p.track_title.is_empty() {
                &p.track_title
            } else if p.track_id > 0 {
                "Waiting for track details"
            } else {
                "No track loaded"
            }
        });
        // Include the artist line in the touch target while keeping the player
        // heading's separate expand action above it.
        let title_link = ui.interact(
            metadata(50.0, 48.0),
            ui.scope_id().with(("inspect-title", number)),
            if local.is_some() {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        let title_link = if local.is_some() {
            title_link
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_text("Inspect this library track")
        } else {
            title_link
        };
        self.hit(&format!("Deck title {number}"), title_link.rect);
        theme::text(
            &painter,
            metadata(50.0, 22.0),
            title,
            17.0,
            if local.is_some() && title_link.hovered() {
                ORANGE
            } else if live {
                TEXT
            } else {
                MUTED
            },
        );
        theme::text(
            &painter,
            metadata(75.0, 18.0),
            player.as_ref().map_or("", |p| p.artist.as_str()),
            13.0,
            MUTED,
        );
        let source = player.as_ref().map_or("", |p| {
            if p.external {
                if p.source.is_empty() {
                    "External source"
                } else {
                    &p.source
                }
            } else if p.track_id > 0 {
                "Vynull library"
            } else {
                "Choose a track from Collection"
            }
        });
        theme::text(&painter, metadata(96.0, 14.0), source, 10.0, ORANGE);
        let bpm = player.as_ref().map_or(0.0, |p| p.bpm);
        theme::text(
            &painter,
            Rect::from_min_size(inside.min + vec2(0.0, 119.0), vec2(125.0, 30.0)),
            if bpm.is_finite() && bpm > 0.0 {
                format!("{bpm:.2}")
            } else {
                "—".into()
            },
            25.0,
            TEXT,
        );
        if let Some(p) = &player {
            theme::text(
                &painter,
                Rect::from_min_size(
                    inside.min + vec2(129.0, 119.0),
                    vec2(inside.width() - 129.0, 20.0),
                ),
                format!("{}   {:+.2}%", p.key, p.pitch_pct),
                12.0,
                ORANGE,
            );
            for beat in 1..=4 {
                painter.rect_filled(
                    Rect::from_min_size(
                        inside.min + vec2(131.0 + f32::from(beat - 1) * 18.0, 141.0),
                        vec2(13.0, 5.0),
                    ),
                    2,
                    if live && p.beat_in_bar == beat {
                        GREEN
                    } else {
                        LINE
                    },
                );
            }
        }
        let wave = Rect::from_min_max(
            inside.min + vec2(0.0, 156.0),
            pos2(inside.right(), inside.bottom() - 80.0),
        );
        paint_overview(
            &painter,
            wave,
            &assets,
            position,
            self.waveform_style,
            self.waveform_overview,
            false,
        );
        self.hit(&format!("Deck waveform {number}"), wave);
        if ui
            .interact(
                wave,
                ui.scope_id().with(("focus-wave", number)),
                Sense::click(),
            )
            .clicked()
        {
            self.deck_focus = Some(number);
        }
        let times = if assets.duration > 0.0 {
            position.map_or(format!("— / {}", time(assets.duration)), |pos| {
                format!(
                    "~{}   /   −{}",
                    time(pos),
                    time((assets.duration - pos).max(0.0))
                )
            })
        } else {
            "Timing unavailable".into()
        };
        let next_cue = timing::cue_summary(&assets, position, false);
        let compact = next_cue.is_some() || !assets.phrases.is_empty();
        let times_rect = Rect::from_min_max(
            pos2(inside.left(), wave.bottom() + 2.0),
            pos2(
                inside.right(),
                wave.bottom() + if compact { 16.0 } else { 22.0 },
            ),
        );
        theme::text(
            &painter,
            if next_cue.is_some() {
                times_rect.with_max_x(times_rect.left() + 124.0)
            } else {
                times_rect
            },
            times,
            if compact { 11.0 } else { 13.0 },
            MUTED,
        );
        if let Some(next_cue) = next_cue {
            theme::text(
                &painter,
                times_rect.with_min_x(times_rect.left() + 128.0),
                next_cue,
                11.0,
                TEXT,
            );
        }
        phrase_ui::paint_summary(
            &painter,
            Rect::from_min_max(
                pos2(inside.left(), wave.bottom() + 16.0),
                pos2(inside.right(), inside.bottom() - 48.0),
            ),
            &assets,
            position,
            11.0,
        );
        let inspect_rect = Rect::from_min_size(
            pos2(inside.left(), inside.bottom() - 48.0),
            vec2(96.0, 48.0),
        );
        let inspect = ui
            .add_enabled_ui(local.is_some(), |ui| {
                ui.put(inspect_rect, Button::new("Inspect").fill(CARD))
            })
            .inner;
        self.hit(&format!("Deck inspect {number}"), inspect.rect);
        if (inspect.clicked() || title_link.clicked())
            && let Some(track) = local
        {
            self.inspect(track);
        }
        let selected = self
            .selected
            .and_then(|id| self.tracks.iter().find(|t| t.id == id))
            .cloned();
        let reason = if !live {
            Some("Waiting for Link")
        } else if self.load_busy || self.pending_load.is_some() {
            Some("Load pending")
        } else if selected.as_ref().is_some_and(|t| t.file_missing) {
            Some("File missing")
        } else {
            peer.as_ref()
                .map_or(Some("Not connected"), |p| self.status.load_error(p))
        };
        let load_rect = Rect::from_min_max(
            pos2(inspect_rect.right() + 8.0, inspect_rect.top()),
            inside.max,
        );
        let enabled = selected.is_none() || reason.is_none();
        let label = if selected.is_none() {
            "Choose track"
        } else {
            reason.unwrap_or("Load selected")
        };
        let load = ui
            .add_enabled_ui(enabled, |ui| {
                ui.put(
                    load_rect,
                    Button::new(RichText::new(label).size(14.0)).fill(
                        if reason.is_none() && selected.is_some() {
                            SELECTED
                        } else {
                            CARD
                        },
                    ),
                )
            })
            .inner;
        self.hit(&format!("Deck load {number}"), load.rect);
        if load.clicked() {
            if let (Some(track), Some(peer)) = (selected, peer) {
                self.load_on_player(track, peer);
            } else {
                self.page = Page::Collection;
                self.notice =
                    format!("Choose a track, then return to Players to load Player {number}");
            }
        }
        if let Some(track) = self.selected_track() {
            load.on_hover_text(format!("Selected: {}", track.title));
        }
    }
}

fn paint_overview(
    p: &Painter,
    rect: Rect,
    assets: &Assets,
    position: Option<f64>,
    style: crate::waveform::Style,
    overview: crate::waveform::Overview,
    shade_played: bool,
) {
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    p.rect_filled(rect, 0, BG);
    let rect = phrase_ui::strip(&p, rect, assets, 0.0..assets.duration, position);
    let use_bands = style.uses_bands(!assets.band_overview.is_empty());
    if assets.overview.is_empty() && !use_bands {
        theme::text(
            &p,
            rect.shrink(6.0),
            if assets.warning.is_empty() {
                "Waiting for waveform"
            } else {
                "Waveform unavailable"
            },
            12.0,
            MUTED,
        );
        crate::waveform::markers::hot_cues(
            &p,
            rect,
            0.0..assets.duration,
            &assets.cues,
            crate::waveform::markers::Edge::Top,
        );
        return;
    }
    let bins = (rect.width() * p.ctx().pixels_per_point()).ceil().max(1.0) as usize;
    if use_bands {
        let columns: Vec<_> = (0..bins)
            .map(|x| {
                let start = x * assets.band_overview.len() / bins;
                let end = ((x + 1) * assets.band_overview.len())
                    .div_ceil(bins)
                    .min(assets.band_overview.len());
                crate::waveform::band_peak(&assets.band_overview[start..end]).unwrap_or_default()
            })
            .collect();
        crate::waveform::half_band_columns(&p, rect.shrink2(vec2(0.0, 2.0)), &columns);
    } else {
        for x in 0..bins {
            let start = x * assets.overview.len() / bins;
            let end = ((x + 1) * assets.overview.len())
                .div_ceil(bins)
                .min(assets.overview.len());
            let spectral = assets.overview[start..end]
                .iter()
                .max_by_key(|e| e.h)
                .copied();
            crate::waveform::column(
                &p,
                Rect::from_min_max(
                    pos2(
                        rect.left() + x as f32 * rect.width() / bins as f32,
                        rect.top() + 2.0,
                    ),
                    pos2(
                        rect.left() + (x + 1) as f32 * rect.width() / bins as f32,
                        rect.bottom() - 2.0,
                    ),
                ),
                style,
                if style == crate::waveform::Style::ThreeBand {
                    crate::waveform::Overview::Half
                } else {
                    overview
                },
                spectral,
                None,
            );
        }
    }
    if assets.duration > 0.0 {
        if shade_played && let Some(pos) = position {
            p.rect_filled(
                Rect::from_min_max(
                    rect.min,
                    pos2(
                        rect.left() + (pos / assets.duration).clamp(0.0, 1.0) as f32 * rect.width(),
                        rect.bottom(),
                    ),
                ),
                0,
                Color32::from_black_alpha(65),
            );
        }
        for cue in assets
            .cues
            .iter()
            .filter(|cue| !(1..=8).contains(&cue.number))
        {
            let phase = f64::from(cue.time_ms) / 1000.0 / assets.duration;
            if (0.0..=1.0).contains(&phase) {
                p.vline(
                    rect.left() + phase as f32 * rect.width(),
                    rect.y_range(),
                    Stroke::new(1.0, crate::cues::color(cue.color_id)),
                );
            }
        }
        crate::waveform::markers::hot_cues(
            &p,
            rect,
            0.0..assets.duration,
            &assets.cues,
            crate::waveform::markers::Edge::Top,
        );
        if let Some(pos) = position {
            p.vline(
                rect.left() + (pos / assets.duration).clamp(0.0, 1.0) as f32 * rect.width(),
                rect.y_range(),
                Stroke::new(2.0, TEXT),
            );
        }
    }
}
