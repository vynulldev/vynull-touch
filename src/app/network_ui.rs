// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

fn kind(peer: &Peer) -> &str {
    if peer.is_mixer() {
        "Mixer"
    } else {
        match peer.device_type.as_str() {
            "cdj" => "Player",
            "rekordbox" => "rekordbox",
            "" | "unknown" => "Unknown device",
            other => other,
        }
    }
}

fn name(peer: &Peer) -> &str {
    if peer.name.trim().is_empty() {
        kind(peer)
    } else {
        &peer.name
    }
}

fn address(value: &str) -> &str {
    if value.is_empty() { "—" } else { value }
}

fn identity(peer: &Peer) -> String {
    format!(
        "{} · Device {} · IP {} · MAC {}",
        kind(peer),
        peer.device_number,
        address(&peer.ip),
        address(&peer.mac)
    )
}

// Discovery and channel state are independent. A presence-only mixer must not
// appear off air, and a stale server snapshot cannot supply current channel state.
fn channels(peer: &Peer, fresh: bool) -> [Option<bool>; 4] {
    let mixer = peer
        .mixer
        .as_ref()
        .filter(|m| fresh && m.channel_state_known);
    std::array::from_fn(|index| mixer.and_then(|m| m.channel_on_air.get(index).copied()))
}

impl App {
    fn network_peers(&self) -> Vec<&Peer> {
        let mut peers: Vec<_> = self.status.peers.iter().collect();
        peers.sort_by(|a, b| {
            (!a.is_mixer(), a.device_number, &a.ip, &a.mac, &a.name).cmp(&(
                !b.is_mixer(),
                b.device_number,
                &b.ip,
                &b.mac,
                &b.name,
            ))
        });
        peers
    }

    pub(super) fn network_indicator(&mut self, ui: &mut Ui) {
        let colors = crate::theme::palette(ui.ctx());
        let fresh = self.fresh();
        let mixers: Vec<_> = self
            .network_peers()
            .into_iter()
            .filter(|p| p.is_mixer())
            .collect();
        let compact = ui.ctx().content_rect().width() < 960.0;
        let label = if !fresh {
            "Mixer ?".into()
        } else {
            match mixers.as_slice() {
                [] => "No mixer".into(),
                [peer] if !compact => name(peer).to_owned(),
                [_] => "1 mixer".into(),
                peers => format!("{} mixers", peers.len()),
            }
        };
        let color = if !fresh {
            ORANGE
        } else if mixers.is_empty() {
            colors.muted
        } else {
            GREEN
        };
        let mut description = if fresh {
            "Open network devices".to_owned()
        } else {
            "Waiting for fresh network status. Open last known devices.".to_owned()
        };
        for peer in mixers {
            description.push_str(&format!("\n{} · {}", name(peer), address(&peer.ip)));
        }
        let response = theme::button(
            ui,
            "",
            vec2(if compact { 68.0 } else { 128.0 }, 48.0),
            false,
        )
        .on_hover_text(&description);
        response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &description));
        let rect = response.rect;
        theme::text(
            ui.painter(),
            Rect::from_min_max(rect.min + vec2(5.0, 2.0), rect.max - vec2(5.0, 24.0)),
            if fresh {
                format!("{} players", self.status.players.len())
            } else {
                "Players ?".into()
            },
            12.0,
            colors.muted,
        );
        ui.painter()
            .circle_filled(pos2(rect.left() + 8.0, rect.bottom() - 13.0), 3.0, color);
        theme::text(
            ui.painter(),
            Rect::from_min_max(rect.min + vec2(15.0, 24.0), rect.max - vec2(4.0, 2.0)),
            label,
            11.0,
            color,
        );
        self.hit("Network devices", rect);
        if response.clicked() {
            self.page = Page::Diag;
            self.diagnostics.tab = diag_ui::Tab::Network;
            self.keyboard = None;
        }
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    fn network_summary(&self) -> String {
        if self.options.demo {
            format!("Demo network · {} devices", self.status.peers.len())
        } else if !self.fresh() {
            "Status unavailable · last known devices".into()
        } else {
            format!(
                "{} devices detected · Link {}",
                self.status.peers.len(),
                if self.linked { "active" } else { "inactive" }
            )
        }
    }

    pub(super) fn network_devices(&self, ui: &mut Ui) {
        let colors = crate::theme::palette(ui.ctx());
        let fresh = self.fresh();
        ui.label(RichText::new(self.network_summary()).color(if fresh {
            colors.muted
        } else {
            ORANGE
        }));
        ScrollArea::vertical()
            .id_salt("network-devices")
            .scroll_source(scroll_area::ScrollSource::ALL)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.status.peers.is_empty() {
                    ui.label(if fresh {
                        "No network devices detected."
                    } else {
                        "Waiting for fresh network status."
                    });
                }
                for peer in self.network_peers() {
                    Frame::new()
                        .fill(colors.panel)
                        .corner_radius(4)
                        .inner_margin(10)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            // These rows are readouts; keep the 48-point touch
                            // minimum on controls without inflating every label.
                            ui.spacing_mut().interact_size.y = 24.0;
                            ui.spacing_mut().item_spacing.y = 6.0;
                            let (heading, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), 24.0),
                                Sense::hover(),
                            );
                            theme::bold_text(
                                ui.painter(),
                                Rect::from_min_max(
                                    heading.min,
                                    pos2(heading.right() - 88.0, heading.bottom()),
                                ),
                                name(peer),
                                17.0,
                                colors.text,
                            );
                            theme::text(
                                ui.painter(),
                                Rect::from_min_max(
                                    pos2(heading.right() - 72.0, heading.top()),
                                    heading.max,
                                ),
                                if fresh { "Detected" } else { "Last seen" },
                                12.0,
                                if fresh { GREEN } else { ORANGE },
                            );
                            ui.add(
                                Label::new(
                                    RichText::new(identity(peer)).size(13.0).color(colors.muted),
                                )
                                .wrap(),
                            );
                            if peer.is_mixer() {
                                let states = channels(peer, fresh);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(
                                        RichText::new("ON AIR").size(11.0).color(colors.muted),
                                    );
                                    for (index, state) in states.iter().enumerate() {
                                        let text = match state {
                                            Some(true) => "ON",
                                            Some(false) => "OFF",
                                            None => "?",
                                        };
                                        Frame::new()
                                            .fill(if *state == Some(true) {
                                                ON_AIR
                                            } else {
                                                colors.card
                                            })
                                            .corner_radius(2)
                                            .inner_margin(Margin::symmetric(8, 4))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    RichText::new(format!(
                                                        "CH{} {text}",
                                                        index + 1
                                                    ))
                                                    .size(12.0)
                                                    .color(if state.is_some() {
                                                        colors.text
                                                    } else {
                                                        colors.muted
                                                    }),
                                                );
                                            });
                                    }
                                });
                                if states.iter().all(Option::is_none) {
                                    ui.label(
                                        RichText::new(if fresh {
                                            "Channel state not reported"
                                        } else {
                                            "Channel state unavailable while status is stale"
                                        })
                                        .size(12.0)
                                        .color(colors.muted),
                                    );
                                }
                            }
                        });
                }
            });
        ui.ctx().request_repaint_after(Duration::from_secs(1));
    }

    pub(super) fn network_report(&self) -> String {
        let fresh = self.fresh();
        let mut report = format!("NETWORK DEVICES\n{}\n", self.network_summary());
        for peer in self.network_peers() {
            report.push_str(&format!(
                "{} · {}\n{}\n",
                name(peer),
                if fresh { "Detected" } else { "Last seen" },
                identity(peer)
            ));
            if peer.is_mixer() {
                let states: Vec<_> = channels(peer, fresh)
                    .iter()
                    .enumerate()
                    .map(|(index, state)| {
                        format!(
                            "CH{}: {}",
                            index + 1,
                            match state {
                                Some(true) => "on air",
                                Some(false) => "off air",
                                None => "unknown",
                            }
                        )
                    })
                    .collect();
                report.push_str(&format!("{}\n", states.join(" · ")));
            }
        }
        report
    }
}
