// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[derive(Clone)]
pub(super) struct Confirmation {
    track: Track,
    peer: Peer,
    playing: Player,
}

#[cfg(test)]
mod tests;

impl App {
    fn load_request_error(&self, track: &Track, peer: &Peer) -> Option<&'static str> {
        if !self.fresh() {
            Some("Waiting for fresh status")
        } else if !self.linked {
            Some("Pro DJ Link is disabled")
        } else if self.load_busy || self.pending_load.is_some() {
            Some("Load pending")
        } else if track.file_missing {
            Some("File missing")
        } else {
            self.status.load_error(peer)
        }
    }

    pub(super) fn load_modal(&mut self, ctx: &Context) {
        let Some(track) = self.selected_track().cloned() else {
            self.load_dialog = false;
            return;
        };
        let response = Modal::new(Id::unique("load-player")).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 64.0).min(440.0);
            ui.set_width(width);
            ui.heading("Load to a player");
            ui.add(Label::new(&track.title).truncate());
            let peers: Vec<_> = self
                .status
                .peers
                .iter()
                .filter(|p| p.device_type == "cdj")
                .cloned()
                .collect();
            if peers.is_empty() {
                ui.label("No players connected.");
            }
            for peer in peers {
                let reason = self.load_request_error(&track, &peer);
                let playing =
                    self.status.players.iter().any(|p| {
                        p.device_number == peer.device_number && p.needs_load_confirmation()
                    });
                let label = format!(
                    "Player {} · {}",
                    peer.device_number,
                    reason.unwrap_or(if playing {
                        "Playing · Confirm load…"
                    } else {
                        "Load track"
                    })
                );
                let response = ui
                    .add_enabled_ui(reason.is_none(), |ui| {
                        theme::button(ui, &label, vec2(width, 56.0), reason.is_none())
                    })
                    .inner;
                self.hit(&format!("Load {}", peer.device_number), response.rect);
                if response.clicked() {
                    self.load_on_player(track.clone(), peer);
                    // Keep the chooser behind confirmation so Cancel returns to it.
                    self.load_dialog = self.load_confirmation.is_some();
                }
            }
            let cancel = theme::button(ui, "Cancel", vec2(width, 48.0), false);
            self.hit("Load picker cancel", cancel.rect);
            if cancel.clicked() {
                self.load_dialog = false;
            }
        });
        self.hit("Load picker", response.response.rect);
        if response.should_close() {
            self.load_dialog = false;
        }
    }

    pub(super) fn confirm_load_modal(&mut self, ctx: &Context) {
        let Some(confirmation) = self.load_confirmation.clone() else {
            return;
        };
        let reason = self
            .load_request_error(&confirmation.track, &confirmation.peer)
            .or_else(|| {
                self.status
                    .load_confirmation_error(&confirmation.peer, Some(&confirmation.playing))
            });
        let on_air = confirmation.playing.on_air
            || self.status.peers.iter().any(|peer| {
                peer.mixer.as_ref().is_some_and(|mixer| {
                    mixer.channel_state_known
                        && mixer
                            .channel_on_air
                            .get(usize::from(
                                confirmation.peer.device_number.saturating_sub(1),
                            ))
                            .copied()
                            .unwrap_or(false)
                })
            });
        let response = Modal::new(Id::unique("confirm-playing-load")).show(ctx, |ui| {
            let width = (ctx.content_rect().width() - 64.0).min(440.0);
            ui.set_width(width);
            ui.heading(format!(
                "Replace track on Player {}?",
                confirmation.peer.device_number
            ));
            ui.label(if on_air {
                "This player is playing and on air. Loading another track will interrupt playback."
            } else {
                "This player is playing. Loading another track will interrupt playback."
            });
            ui.add_space(4.0);
            ui.label(RichText::new("CURRENT TRACK").size(11.0).color(MUTED));
            ui.add(
                Label::new(if confirmation.playing.track_title.is_empty() {
                    "Unknown track"
                } else {
                    &confirmation.playing.track_title
                })
                .truncate(),
            );
            ui.label(RichText::new("LOAD INSTEAD").size(11.0).color(MUTED));
            ui.add(Label::new(RichText::new(&confirmation.track.title).color(ORANGE)).truncate());
            ui.add(
                Label::new(
                    RichText::new(&confirmation.track.artist)
                        .size(13.0)
                        .color(MUTED),
                )
                .truncate(),
            );
            if let Some(reason) = reason {
                ui.label(RichText::new(reason).color(ORANGE));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let cancel = theme::button(ui, "Cancel", vec2(136.0, 48.0), false);
                self.hit("Load cancel", cancel.rect);
                if cancel.clicked() {
                    self.load_confirmation = None;
                }
                let confirm = ui
                    .add_enabled_ui(reason.is_none(), |ui| {
                        theme::primary(ui, "Load track", vec2(width - 144.0, 48.0))
                    })
                    .inner;
                self.hit("Load confirm", confirm.rect);
                if confirm.clicked() {
                    self.load_confirmation = None;
                    self.load_dialog = false;
                    self.submit_load(
                        confirmation.track.clone(),
                        confirmation.peer.clone(),
                        Some(confirmation.playing.clone()),
                    );
                }
            });
        });
        self.hit("Load confirmation", response.response.rect);
        if response.should_close() {
            self.load_confirmation = None;
        }
    }

    pub(super) fn load_on_player(&mut self, track: Track, peer: Peer) {
        if let Some(reason) = self.load_request_error(&track, &peer) {
            self.notice = reason.into();
            return;
        }
        if let Some(playing) = self
            .status
            .players
            .iter()
            .find(|p| p.device_number == peer.device_number && p.needs_load_confirmation())
            .cloned()
        {
            self.load_confirmation = Some(Confirmation {
                track,
                peer,
                playing,
            });
        } else {
            self.submit_load(track, peer, None);
        }
    }

    fn submit_load(&mut self, track: Track, peer: Peer, confirmed: Option<Player>) {
        if let Some(reason) = self.load_request_error(&track, &peer).or_else(|| {
            self.status
                .load_confirmation_error(&peer, confirmed.as_ref())
        }) {
            self.notice = reason.into();
            return;
        }
        if self.options.demo {
            if let Some(player) = self
                .status
                .players
                .iter_mut()
                .find(|p| p.device_number == peer.device_number)
            {
                *player = Player {
                    device_number: peer.device_number,
                    name: peer.name.clone(),
                    track_id: track.id,
                    track_title: track.title.clone(),
                    artist: track.artist.clone(),
                    bpm: track.bpm,
                    key: track.key.clone(),
                    on_air: player.on_air,
                    is_master: player.is_master,
                    play_state: 6,
                    play_state_name: "CUED".into(),
                    beat_in_track: 1,
                    beat_in_bar: 1,
                    duration_ms: track.duration.saturating_mul(1000),
                    ..Default::default()
                };
            }
            self.notice = format!(
                "Demo · {} loaded to Player {}",
                track.title, peer.device_number
            );
        } else {
            self.load_busy = self.send(Command::Load {
                track: Box::new(track),
                peer,
                confirmed: confirmed.map(Box::new),
            });
        }
    }
}
