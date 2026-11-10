// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::ops::Range;

impl App {
    pub(super) fn player_focus(&mut self, ui: &mut Ui, number: u8) {
        let DeckSnapshot {
            peer,
            player,
            local,
            view,
            assets,
            position,
            waveform_position,
            live,
        } = self.deck_snapshot(number);
        ui.horizontal(|ui| {
            let back = theme::button(
                ui,
                if self.deck_compare {
                    "‹ Waveforms"
                } else {
                    "‹ Overview"
                },
                vec2(112.0, 48.0),
                false,
            );
            self.hit("Player overview", back.rect);
            if back.clicked() {
                self.deck_focus = None;
            }
            let mut numbers: Vec<_> = (1..=4).chain(self.deck_pair).chain([number]).collect();
            numbers.sort_unstable();
            numbers.dedup();
            for n in numbers {
                let button = theme::button(ui, &n.to_string(), vec2(48.0, 48.0), n == number);
                self.hit(&format!("Player select {n}"), button.rect);
                if button.clicked() {
                    self.deck_focus = Some(n);
                }
            }
            let (rect, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
            theme::text(
                ui.painter(),
                rect.shrink2(vec2(8.0, 0.0)),
                format!(
                    "PLAYER {number:02} · {}",
                    player_state(self.fresh(), self.linked, peer.as_ref(), player.as_ref())
                ),
                13.0,
                if live { GREEN } else { MUTED },
            );
        });
        let (metadata, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 72.0), Sense::hover());
        let p = ui.painter().with_clip_rect(metadata);
        let art = Rect::from_min_size(metadata.min, vec2(68.0, 68.0));
        if let Some(texture) = view.art {
            p.image(
                texture.id(),
                art,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        } else {
            theme::sleeve(
                &p,
                art,
                player.as_ref().map_or(u32::from(number), |p| p.track_id),
            );
        }
        let text_rect = |y, height| {
            Rect::from_min_size(
                metadata.min + vec2(80.0, y),
                vec2(metadata.width() - 260.0, height),
            )
        };
        let title = player.as_ref().map_or("No player connected", |p| {
            if p.track_id == 0 {
                "No track loaded"
            } else if p.track_title.is_empty() {
                "Waiting for track details"
            } else {
                &p.track_title
            }
        });
        theme::text(&p, text_rect(0.0, 27.0), title, 22.0, TEXT);
        theme::text(
            &p,
            text_rect(29.0, 20.0),
            player.as_ref().map_or("", |p| p.artist.as_str()),
            14.0,
            MUTED,
        );
        let source = player
            .as_ref()
            .map_or("Waiting for a Pro DJ Link player", |p| {
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
        theme::text(&p, text_rect(53.0, 16.0), source, 11.0, ORANGE);
        let right = Rect::from_min_size(
            pos2(metadata.right() - 172.0, metadata.top()),
            vec2(172.0, 30.0),
        );
        let bpm = player.as_ref().map_or(0.0, |p| p.bpm);
        theme::text(
            &p,
            right,
            if bpm.is_finite() && bpm > 0.0 {
                format!("{bpm:.2} BPM")
            } else {
                "— BPM".into()
            },
            24.0,
            TEXT,
        );
        if let Some(player) = &player {
            theme::text(
                &p,
                right
                    .translate(vec2(0.0, 28.0))
                    .with_max_y(metadata.top() + 49.0),
                format!("{}   {:+.2}%", player.key, player.pitch_pct),
                14.0,
                ORANGE,
            );
            let flags = if live {
                [
                    player.is_master.then_some("MASTER"),
                    player.is_sync.then_some("SYNC"),
                    player.on_air.then_some("ON AIR"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            } else {
                String::new()
            };
            theme::text(&p, right.translate(vec2(0.0, 53.0)), flags, 11.0, GREEN);
        }
        // Grow the overview for its phrase strip where the remaining detailed
        // waveform and 48px controls can still fit on the compact screen.
        let extra = if assets.phrases.is_empty() {
            0.0
        } else {
            (ui.available_height() - 40.0 - 8.0 - 88.0 - 24.0 - 48.0 - 16.0)
                .clamp(0.0, crate::phrases::STRIP_HEIGHT)
        };
        let (overview, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), 40.0 + extra), Sense::hover());
        paint_overview(
            ui.painter(),
            overview,
            &assets,
            position,
            self.waveform_style,
            self.waveform_overview,
            false,
        );
        self.hit("Player overview waveform", overview);
        let range = waveform_position.map(|pos| detail_range(pos, self.deck_span));
        if let Some(range) = &range
            && assets.duration > 0.0
        {
            let left = overview.left()
                + (range.start / assets.duration).clamp(0.0, 1.0) as f32 * overview.width();
            let right = overview.left()
                + (range.end / assets.duration).clamp(0.0, 1.0) as f32 * overview.width();
            ui.painter().rect_stroke(
                Rect::from_x_y_ranges(left..=right, overview.y_range()),
                1,
                Stroke::new(1.5, ORANGE),
                StrokeKind::Inside,
            );
        }
        let height = (ui.available_height() - 24.0 - 48.0 - 16.0).max(88.0);
        let (wave, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        paint_detail(
            ui.painter(),
            wave,
            &assets,
            range,
            position,
            self.waveform_style,
            0.0,
        );
        self.hit("Player detail waveform", wave);
        let (times, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let timing = position.map_or("Position unavailable".into(), |pos| {
            if assets.duration > 0.0 {
                format!(
                    "~{}   /   −{}",
                    time(pos),
                    time((assets.duration - pos).max(0.0))
                )
            } else {
                format!("~{}", time(pos))
            }
        });
        theme::text(
            ui.painter(),
            Rect::from_min_size(times.min, vec2(220.0, 24.0)),
            timing,
            18.0,
            TEXT,
        );
        let next = position.and_then(|pos| next_cue(&assets, pos));
        let hint = next.map_or_else(
            || {
                if position.is_none() {
                    if waveform_position.is_some_and(|pos| pos > 0.0) {
                        "Waveform at last known position".into()
                    } else {
                        "Track start · waiting for player timing".into()
                    }
                } else if assets.beats.is_empty() {
                    "Estimated from BPM · no saved grid".into()
                } else {
                    "Estimated from player beat position".into()
                }
            },
            |(cue, distance)| {
                format!(
                    "Next {} · {}   +{}",
                    crate::cues::slot(cue.number),
                    cue.label,
                    time(distance)
                )
            },
        );
        theme::text(
            ui.painter(),
            Rect::from_min_max(times.min + vec2(228.0, 0.0), times.max),
            hint,
            12.0,
            MUTED,
        );
        ui.horizontal(|ui| {
            let inspect = ui
                .add_enabled_ui(local.is_some(), |ui| {
                    theme::button(ui, "Inspect", vec2(96.0, 48.0), false)
                })
                .inner;
            self.hit("Player inspect", inspect.rect);
            if inspect.clicked()
                && let Some(track) = local
            {
                self.inspect(track);
            }
            let selected = self.selected_track().cloned();
            let ready = live
                && !self.load_busy
                && self.pending_load.is_none()
                && selected.as_ref().is_some_and(|t| !t.file_missing)
                && peer
                    .as_ref()
                    .is_some_and(|p| self.status.load_error(p).is_none());
            let label = if selected.is_none() {
                "Choose track"
            } else if !live {
                "Waiting for Link"
            } else if self.load_busy || self.pending_load.is_some() {
                "Load pending"
            } else if selected.as_ref().is_some_and(|t| t.file_missing) {
                "File missing"
            } else {
                peer.as_ref()
                    .and_then(|p| self.status.load_error(p))
                    .unwrap_or("Load selected")
            };
            let load = ui
                .add_enabled_ui(selected.is_none() || ready, |ui| {
                    theme::button(ui, label, vec2(176.0, 48.0), ready)
                })
                .inner;
            self.hit("Player load", load.rect);
            if load.clicked() {
                if let (Some(track), Some(peer)) = (selected, peer) {
                    self.load_on_player(track, peer);
                } else {
                    self.page = Page::Collection;
                    self.notice = format!("Choose a track, then return to Player {number}");
                }
            }
            let (label, _) = ui.allocate_exact_size(vec2(128.0, 48.0), Sense::hover());
            theme::text(
                ui.painter(),
                label,
                format!("Detail · {} s", self.deck_span as u32),
                14.0,
                MUTED,
            );
            for (name, factor) in [("−", 2.0), ("+", 0.5)] {
                let span = (self.deck_span * factor).clamp(4.0, 64.0);
                let zoom = ui
                    .add_enabled_ui(span != self.deck_span, |ui| {
                        theme::button(ui, name, vec2(48.0, 48.0), false)
                    })
                    .inner;
                self.hit(&format!("Player zoom {name}"), zoom.rect);
                if zoom.clicked() {
                    self.deck_span = span;
                    self.save_players();
                }
            }
        });
        if live && player.as_ref().is_some_and(|p| p.is_playing) {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }
}

pub(super) fn player_state<'a>(
    fresh: bool,
    linked: bool,
    peer: Option<&Peer>,
    player: Option<&'a Player>,
) -> &'a str {
    if !fresh {
        "STALE"
    } else if !linked {
        "LINK OFF"
    } else if peer.is_none() {
        "DISCONNECTED"
    } else {
        player.map_or("WAITING", |p| {
            if p.play_state_name.is_empty() {
                "UNKNOWN"
            } else {
                &p.play_state_name
            }
        })
    }
}

pub(super) fn detail_range(position: f64, span: f64) -> Range<f64> {
    (position - span * 0.35)..(position + span * 0.65)
}

pub(super) fn next_cue(assets: &Assets, position: f64) -> Option<(&Cue, f64)> {
    assets
        .cues
        .iter()
        .filter(|cue| f64::from(cue.time_ms) / 1000.0 > position)
        .min_by_key(|cue| cue.time_ms)
        .map(|cue| (cue, f64::from(cue.time_ms) / 1000.0 - position))
}

fn peak(assets: &Assets, start: f64, end: f64) -> Option<WaveEntry> {
    let rate = f64::from(assets.sample_rate);
    let first = (start.max(0.0) * rate).floor() as usize;
    let last = ((end.max(0.0) * rate).ceil() as usize).min(assets.detail.len());
    assets
        .detail
        .get(first..last)?
        .iter()
        .max_by_key(|e| e.h)
        .copied()
}

pub(super) fn paint_detail(
    p: &Painter,
    rect: Rect,
    assets: &Assets,
    range: Option<Range<f64>>,
    position: Option<f64>,
    style: crate::waveform::Style,
    header_right: f32,
) {
    let p = p.with_clip_rect(rect.intersect(p.clip_rect()));
    p.rect_filled(rect, 4, BG);
    let rect = if assets.phrases.is_empty() {
        rect
    } else {
        let header = rect.with_max_y(rect.top() + phrase_ui::HEADER_HEIGHT);
        phrase_ui::paint_summary(
            &p,
            header
                .shrink2(vec2(4.0, 0.0))
                .with_max_x(header.right() - header_right - 4.0),
            assets,
            position,
            12.0,
        );
        rect.with_min_y(header.bottom())
    };
    let Some(range) = range else {
        theme::text(
            &p,
            rect.shrink(12.0),
            "Waiting for player timing",
            15.0,
            MUTED,
        );
        return;
    };
    let rect = phrase_ui::strip(&p, rect, assets, range.clone(), position);
    let use_bands = style.uses_bands(assets.bands.available());
    if !use_bands && (assets.detail.is_empty() || assets.sample_rate == 0) {
        theme::text(
            &p,
            rect.shrink(12.0),
            if assets.warning.is_empty() {
                "Loading detail waveform…"
            } else {
                "Detail waveform unavailable"
            },
            15.0,
            MUTED,
        );
        crate::waveform::markers::hot_cues(
            &p,
            rect,
            range,
            &assets.cues,
            crate::waveform::markers::Edge::Bottom,
        );
        return;
    }
    let span = range.end - range.start;
    let x = |seconds: f64| rect.left() + ((seconds - range.start) / span) as f32 * rect.width();
    let bins = (rect.width() * p.ctx().pixels_per_point())
        .ceil()
        .clamp(1.0, 4096.0) as usize;
    let columns: Vec<_> = (0..bins)
        .map(|i| {
            let start = range.start + span * i as f64 / bins as f64;
            let end = range.start + span * (i + 1) as f64 / bins as f64;
            let (spectral, bands) = if use_bands {
                (None, assets.bands.peak(start, end))
            } else {
                (peak(assets, start, end), None)
            };
            crate::waveform::envelope::Column { spectral, bands }
        })
        .collect();
    let sample_rate = if use_bands {
        assets.bands.sample_rate
    } else {
        assets.sample_rate
    };
    crate::waveform::envelope::paint(
        &p,
        rect.shrink2(vec2(
            0.0,
            if !assets.phrases.is_empty() && rect.height() < 72.0 {
                4.0
            } else {
                12.0
            },
        )),
        style,
        &columns,
        span * f64::from(sample_rate) > bins as f64,
    );
    let budget = (rect.width() / 12.0).max(1.0) as usize;
    if !assets.grid.is_empty() {
        for beat in assets.grid.visible(range.start, range.end, budget) {
            let down = beat.beat_in_bar == 1;
            p.vline(
                x(beat.seconds()),
                rect.y_range(),
                Stroke::new(
                    1.0,
                    if down {
                        Color32::from_rgb(238, 115, 160)
                    } else {
                        Color32::from_white_alpha(65)
                    },
                ),
            );
            if down {
                p.text(
                    pos2(x(beat.seconds()) + 3.0, rect.bottom() - 2.0),
                    Align2::LEFT_BOTTOM,
                    "1",
                    FontId::proportional(9.0),
                    MUTED,
                );
            }
        }
    } else {
        // External analysis supplies timestamps but no authoritative bar phase.
        let first = assets.beats.partition_point(|b| *b < range.start.max(0.0));
        let last = assets.beats.partition_point(|b| *b <= range.end);
        for beat in assets.beats[first..last.max(first)]
            .iter()
            .step_by((last.saturating_sub(first)).div_ceil(budget).max(1))
        {
            p.vline(
                x(*beat),
                rect.y_range(),
                Stroke::new(1.0, Color32::from_white_alpha(65)),
            );
        }
    }
    for cue in &assets.cues {
        let seconds = f64::from(cue.time_ms) / 1000.0;
        let color = crate::cues::color(cue.color_id);
        if cue.kind == "loop"
            && let Ok(region) = crate::loops::Region::from_cue(cue, assets.duration)
            && region.end >= range.start
            && region.start <= range.end
        {
            p.rect_filled(
                Rect::from_x_y_ranges(
                    x(region.start.max(range.start))..=x(region.end.min(range.end)),
                    rect.y_range(),
                ),
                0,
                color.gamma_multiply(0.12),
            );
            if range.contains(&region.end) {
                p.vline(x(region.end), rect.y_range(), Stroke::new(1.0, color));
            }
        }
        if range.contains(&seconds) && !(1..=8).contains(&cue.number) {
            p.vline(x(seconds), rect.y_range(), Stroke::new(1.5, color));
            p.text(
                pos2(x(seconds) + 3.0, rect.top() + 2.0),
                Align2::LEFT_TOP,
                crate::cues::slot(cue.number),
                FontId::proportional(11.0),
                color,
            );
        }
    }
    crate::waveform::markers::hot_cues(
        &p,
        rect,
        range.clone(),
        &assets.cues,
        crate::waveform::markers::Edge::Bottom,
    );
    if let Some(position) = position {
        p.vline(x(position), rect.y_range(), Stroke::new(2.0, TEXT));
    } else {
        p.text(
            rect.left_top() + vec2(8.0, 2.0),
            Align2::LEFT_TOP,
            "Position unavailable",
            FontId::proportional(10.0),
            MUTED,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detail_uses_original_samples_clips_edges_and_finds_upcoming_cues() {
        let mut assets = Assets::demo(&crate::demo::tracks()[0]);
        assets.detail = vec![
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
        ]
        .into();
        assets.sample_rate = 150;
        assets.overview = vec![WaveEntry::default()];
        assert_eq!(peak(&assets, 0.0, 0.02).unwrap().h, 31);
        assert_eq!(peak(&assets, 0.014, 0.02).unwrap().h, 4);
        assert!(peak(&assets, -2.0, -1.0).is_none());
        assert!(peak(&assets, 1.0, 2.0).is_none());
        let range = detail_range(0.0, 16.0);
        assert!(range.start < 0.0 && range.end > 0.0);
        assert_eq!(range.end - range.start, 16.0);
        assert_eq!(next_cue(&assets, 1.0).unwrap().0.number, 2);
        assert_eq!(next_cue(&assets, 32.0).unwrap().0.number, 3);
        assert!(next_cue(&assets, 300.0).is_none());
    }
}
