// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const GAP: f32 = 4.0;

impl App {
    pub(super) fn player_compare(&mut self, ui: &mut Ui, numbers: [u8; 2]) {
        // Sample each deck once: its scrolling and overview waveforms must share
        // the same clock observation, source identity and analysis snapshot.
        let decks = numbers.map(|number| self.deck_snapshot(number));
        let available = ui.available_height();
        let card_height = (available * 0.45).clamp(176.0, 192.0);
        let lane_height = ((available - card_height - 2.0 * GAP) / 2.0).clamp(84.0, 180.0);
        let rail_width = if ui.available_width() >= 900.0 {
            104.0
        } else {
            88.0
        };
        ScrollArea::vertical()
            .id_salt("waveform-players")
            .auto_shrink([false, false])
            .scroll_source(scroll_area::ScrollSource::ALL)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                let mut waves = Vec::with_capacity(2);
                for (number, deck) in numbers.into_iter().zip(&decks) {
                    let (rect, open) = ui.allocate_exact_size(
                        vec2(ui.available_width(), lane_height),
                        Sense::click(),
                    );
                    self.hit(&format!("Compare player {number}"), rect);
                    let wave = self.compare_lane(ui, rect, rail_width, number, deck);
                    waves.push(wave);
                    if open.clicked() {
                        self.deck_focus = Some(number);
                    }
                }
                // Join the two valid clocks across the gap. Each lane draws its
                // own playhead below the phrase header so text stays readable.
                // Missing or stale timing never receives a fabricated playhead.
                if decks.iter().all(|deck| deck.position.is_some()) {
                    let x = waves[0].left() + waves[0].width() * 0.35;
                    ui.painter().vline(
                        x,
                        waves[0].bottom()..=waves[1].top(),
                        Stroke::new(2.0, TEXT),
                    );
                }
                let width = (ui.available_width() - GAP) / 2.0;
                ui.horizontal_top(|ui| {
                    for (number, deck) in numbers.into_iter().zip(&decks) {
                        let (rect, _) =
                            ui.allocate_exact_size(vec2(width, card_height), Sense::hover());
                        self.compare_overview(ui, rect, number, deck);
                    }
                });
            });
        if self.fresh()
            && self.linked
            && self.status.players.iter().any(Player::is_forward_playback)
        {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }
    }

    fn compare_lane(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        rail_width: f32,
        number: u8,
        deck: &DeckSnapshot,
    ) -> Rect {
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0, BG);
        let rail = Rect::from_min_max(rect.min, pos2(rect.left() + rail_width, rect.bottom()));
        let master = deck.live && deck.player.as_ref().is_some_and(|player| player.is_master);
        p.rect_filled(
            Rect::from_min_size(rail.min, vec2(3.0, rail.height())),
            0,
            if master { ORANGE } else { BLUE },
        );
        let inside = rail.shrink2(vec2(8.0, 4.0));
        let art = Rect::from_min_size(pos2(inside.right() - 16.0, inside.top()), vec2(16.0, 16.0));
        if let Some(texture) = &deck.view.art {
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
                deck.player
                    .as_ref()
                    .map_or(u32::from(number), |player| player.track_id),
            );
        }
        theme::text(
            &p,
            Rect::from_min_max(inside.min, pos2(art.left() - 4.0, inside.top() + 23.0)),
            format!("DECK {number}"),
            12.0,
            CYAN,
        );
        let source = deck.player.as_ref().map_or("No player", |player| {
            if deck.peer.is_none() {
                "Disconnected"
            } else if player.track_id == 0 {
                "No track"
            } else if player.external {
                if player.source.is_empty() {
                    "External"
                } else {
                    &player.source
                }
            } else {
                "Library"
            }
        });
        let line = |y: f32, height: f32| {
            Rect::from_min_size(
                pos2(inside.left(), inside.top() + y),
                vec2(inside.width(), height),
            )
        };
        theme::text(&p, line(24.0, 16.0), source, 11.0, MUTED);
        theme::text(
            &p,
            line(42.0, 22.0),
            deck.player
                .as_ref()
                .filter(|p| !p.key.is_empty())
                .map_or("—", |p| p.key.as_str()),
            19.0,
            TEXT,
        );
        let state = focus::player_state(
            self.fresh(),
            self.linked,
            deck.peer.as_ref(),
            deck.player.as_ref(),
        );
        theme::text(
            &p,
            line(65.0, 16.0),
            state,
            10.0,
            if deck.live { MUTED } else { ORANGE },
        );
        let wave = Rect::from_min_max(pos2(rail.right() + 4.0, rect.top()), rect.max);
        let rate = deck
            .player
            .as_ref()
            .filter(|player| player.pitch_pct.is_finite())
            .map_or(1.0, |player| {
                (1.0 + player.pitch_pct / 100.0).clamp(0.01, 3.0)
            });
        // Equal playback-time windows remain aligned even at different pitches.
        let range = deck
            .waveform_position
            .map(|position| focus::detail_range(position, self.deck_span * rate));
        focus::paint_detail(
            &p,
            wave,
            &deck.assets,
            range,
            deck.position,
            self.waveform_style,
            166.0,
        );
        self.hit(&format!("Compare waveform {number}"), wave);
        if deck.position.is_some() {
            let x = wave.left() + wave.width() * 0.35;
            self.hit(
                &format!("Compare playhead {number}"),
                Rect::from_min_max(pos2(x, wave.top()), pos2(x + 2.0, wave.bottom())),
            );
        }
        let flags = if deck.live {
            deck.player.as_ref().map_or(String::new(), |player| {
                [
                    player.is_master.then_some("MASTER"),
                    player.is_sync.then_some("SYNC"),
                    player.on_air.then_some("ON AIR"),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ")
            })
        } else {
            String::new()
        };
        if !flags.is_empty() {
            let badge = Rect::from_min_size(
                pos2(wave.right() - 162.0, wave.top() + 2.0),
                vec2(158.0, 16.0),
            );
            p.rect_filled(badge, 2, BG);
            theme::text(
                &p,
                badge.shrink2(vec2(4.0, 0.0)),
                flags,
                10.0,
                if master { ORANGE } else { GREEN },
            );
        }
        wave
    }

    fn compare_overview(&mut self, ui: &mut Ui, rect: Rect, number: u8, deck: &DeckSnapshot) {
        self.hit(&format!("Compare card {number}"), rect);
        let p = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        p.rect_filled(rect, 0, BG);
        p.rect_stroke(rect, 0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let inside = rect.shrink(8.0);
        let player = deck.player.as_ref();
        let master = deck.live && player.is_some_and(|player| player.is_master);
        let on_air = deck.live && player.is_some_and(|player| player.on_air);
        let header = inside.with_max_y(inside.top() + 32.0);
        p.rect_filled(header, 0, PANEL);
        let badge = header.with_max_x(header.left() + 30.0);
        p.rect_filled(badge, 0, if deck.live { BLUE } else { CARD });
        p.text(
            badge.center(),
            Align2::CENTER_CENTER,
            number.to_string(),
            FontId::proportional(24.0),
            TEXT,
        );
        let metadata = Rect::from_min_max(
            inside.min + vec2(38.0, 0.0),
            pos2(inside.right(), inside.top() + 48.0),
        );
        let title = player.map_or("No player connected", |player| {
            if player.track_id == 0 {
                "No track loaded"
            } else if player.track_title.is_empty() {
                "Waiting for track details"
            } else {
                &player.track_title
            }
        });
        theme::text(
            &p,
            metadata.with_max_y(metadata.top() + 19.0),
            title,
            17.0,
            TEXT,
        );
        theme::text(
            &p,
            Rect::from_min_max(
                metadata.min + vec2(0.0, 19.0),
                pos2(metadata.right(), header.bottom()),
            ),
            player.map_or("", |player| player.artist.as_str()),
            11.0,
            MUTED,
        );
        let title_link = ui
            .add_enabled_ui(deck.local.is_some(), |ui| {
                ui.interact(
                    metadata,
                    ui.make_persistent_id(("compare-title", number)),
                    Sense::click(),
                )
            })
            .inner;
        self.hit(&format!("Compare title {number}"), metadata);
        if title_link.clicked()
            && let Some(track) = &deck.local
        {
            self.inspect(track.clone());
        }
        if deck.local.is_some() {
            title_link.on_hover_text("Inspect this library track");
        }

        let remaining = deck
            .position
            .filter(|_| deck.assets.duration > 0.0)
            .map_or("—:—".into(), |position| {
                format!("~−{}", time((deck.assets.duration - position).max(0.0)))
            });
        let readouts = Rect::from_min_max(
            inside.min + vec2(0.0, 34.0),
            pos2(inside.right(), inside.top() + 76.0),
        );
        let time_rect = readouts.with_max_x(readouts.left() + 132.0);
        theme::text(
            &p,
            time_rect.with_max_y(time_rect.top() + 12.0),
            "REMAIN",
            10.0,
            MUTED,
        );
        theme::text(
            &p,
            time_rect.with_min_y(time_rect.top() + 12.0),
            remaining,
            27.0,
            TEXT,
        );
        self.hit(&format!("Compare time {number}"), time_rect);
        let bpm_rect = readouts.with_min_x(readouts.right() - 78.0);
        let label = bpm_rect.with_max_y(bpm_rect.top() + 12.0);
        if master {
            p.rect_filled(label, 0, ORANGE);
        }
        theme::text(
            &p,
            label.shrink2(vec2(2.0, 0.0)),
            if master { "BPM · MASTER" } else { "BPM" },
            10.0,
            if master { BG } else { MUTED },
        );
        theme::text(
            &p,
            bpm_rect.with_min_y(bpm_rect.top() + 12.0),
            player
                .filter(|p| p.bpm.is_finite() && p.bpm > 0.0)
                .map_or("—".into(), |p| format!("{:.1}", p.bpm)),
            27.0,
            if master { ORANGE } else { TEXT },
        );
        self.hit(&format!("Compare BPM {number}"), bpm_rect);
        let tempo_rect = Rect::from_min_max(
            pos2(time_rect.right() + 4.0, readouts.top()),
            pos2(bpm_rect.left() - 8.0, readouts.bottom()),
        );
        theme::text(
            &p,
            tempo_rect.with_max_y(tempo_rect.top() + 12.0),
            "TEMPO",
            10.0,
            MUTED,
        );
        theme::text(
            &p,
            tempo_rect.with_min_y(tempo_rect.top() + 12.0),
            player
                .filter(|p| p.pitch_pct.is_finite())
                .map_or("—".into(), |p| format!("{:+.2}%", p.pitch_pct)),
            16.0,
            TEXT,
        );

        let wave = Rect::from_min_max(
            inside.min + vec2(0.0, 80.0),
            pos2(
                inside.right(),
                (inside.bottom() - 16.0).min(inside.top() + 160.0),
            ),
        );
        paint_overview(
            &p,
            wave,
            &deck.assets,
            deck.position,
            self.waveform_style,
            self.waveform_overview,
            true,
        );
        // Keep the memory-cue triangles distinct from the lettered hot cues.
        if deck.assets.duration > 0.0 {
            let p = p.with_clip_rect(wave.intersect(p.clip_rect()));
            for cue in deck
                .assets
                .cues
                .iter()
                .filter(|cue| !(1..=8).contains(&cue.number))
            {
                let phase = f64::from(cue.time_ms) / 1000.0 / deck.assets.duration;
                if (0.0..=1.0).contains(&phase) {
                    let x = wave.left() + phase as f32 * wave.width();
                    p.add(Shape::convex_polygon(
                        vec![
                            pos2(x - 4.0, wave.top()),
                            pos2(x + 4.0, wave.top()),
                            pos2(x, wave.top() + 6.0),
                        ],
                        crate::cues::color(cue.color_id),
                        Stroke::NONE,
                    ));
                }
            }
        }
        self.hit(&format!("Compare overview {number}"), wave);
        // The overview painter retains cue flags, memory cues, the played region
        // and phrases; its saved Half/Full preference is unchanged.
        let footer = inside.with_min_y(inside.bottom() - 14.0);
        let state_rect = footer.with_max_x(footer.left() + 66.0);
        let state = focus::player_state(self.fresh(), self.linked, deck.peer.as_ref(), player);
        if on_air {
            p.rect_filled(state_rect, 0, ON_AIR);
        }
        theme::text(
            &p,
            state_rect.shrink2(vec2(3.0, 0.0)),
            if on_air { "ON AIR" } else { state },
            10.0,
            if deck.live { TEXT } else { MUTED },
        );
        let next_cue = timing::cue_summary(&deck.assets, deck.position, false);
        theme::text(
            &p,
            footer.with_min_x(state_rect.right() + 8.0),
            next_cue.as_deref().unwrap_or("Cue countdown unavailable"),
            11.0,
            if next_cue.is_some() { TEXT } else { MUTED },
        );
        if ui
            .interact(
                Rect::from_min_max(pos2(rect.left(), metadata.bottom()), rect.max),
                ui.make_persistent_id(("compare-overview", number)),
                Sense::click(),
            )
            .clicked()
        {
            self.deck_focus = Some(number);
        }
    }
}
