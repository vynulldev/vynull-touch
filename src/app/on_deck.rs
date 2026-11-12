// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::matching::live::Reference;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Playing,
    Cued,
    Paused,
    Ended,
    Loaded,
}

impl State {
    fn label(self) -> &'static str {
        match self {
            Self::Playing => "Playing",
            Self::Cued => "Cued",
            Self::Paused => "Paused",
            Self::Ended => "Ended",
            Self::Loaded => "Loaded",
        }
    }
}

struct Loaded {
    number: u8,
    track: u32,
    path: String,
    state: State,
}

/// Resolve at most four players once per frame, never once per library row.
/// Keep the canonical file identity so an old playlist row cannot match a reused ID.
pub(super) struct Snapshot([Option<Loaded>; 4]);

impl Snapshot {
    fn new(status: &Status, tracks: &[Track], live: bool) -> Self {
        Self(std::array::from_fn(|index| {
            let number = index as u8 + 1;
            let player = Reference::Player(number).player(status, live)?;
            if player.external
                || !player.source.is_empty()
                || player.analysis_url.starts_with("/api/analysis/ext/")
                || player.artwork_url.starts_with("/api/artwork/ext/")
                || player.track_id == 0
                || matches!(player.play_state, 0 | 2)
            {
                return None;
            }
            let mut candidates = tracks.iter().filter(|t| t.id == player.track_id);
            let track = candidates.next()?;
            if candidates.next().is_some() {
                return None;
            }
            let state = if player.needs_load_confirmation() {
                State::Playing
            } else {
                match player.play_state {
                    5 => State::Paused,
                    6 => State::Cued,
                    0x11 => State::Ended,
                    _ => State::Loaded,
                }
            };
            Some(Loaded {
                number,
                track: track.id,
                path: track.file_path.clone(),
                state,
            })
        }))
    }

    fn players<'a>(&'a self, track: &'a Track) -> impl Iterator<Item = &'a Loaded> {
        self.0
            .iter()
            .flatten()
            .filter(|loaded| loaded.track == track.id && loaded.path == track.file_path)
    }

    pub(super) fn contains(&self, track: &Track) -> bool {
        self.players(track).next().is_some()
    }
}

impl App {
    pub(super) fn on_deck_snapshot(&self) -> Snapshot {
        Snapshot::new(
            &self.status,
            &self.all_tracks,
            self.library_ready && self.fresh() && self.linked,
        )
    }

    /// Informational badges use hover sense only: the existing row owns taps/drags.
    /// Return remaining space for Prepare's artist text beside the badges.
    pub(super) fn paint_on_deck(
        &mut self,
        ui: &Ui,
        mut rect: Rect,
        track: &Track,
        snapshot: &Snapshot,
    ) -> Rect {
        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
        for loaded in snapshot.players(track) {
            let badge =
                Rect::from_min_size(pos2(rect.left(), rect.center().y - 9.0), vec2(34.0, 18.0));
            let active = loaded.state == State::Playing;
            let color = if active {
                GREEN
            } else if matches!(loaded.state, State::Cued | State::Paused) {
                ORANGE
            } else {
                MUTED
            };
            painter.rect(
                badge,
                3,
                if active { GREEN } else { CARD },
                Stroke::new(1.0, color),
                StrokeKind::Inside,
            );
            let ink = if active { BG } else { color };
            theme::bold_text(
                &painter,
                Rect::from_min_max(badge.min + vec2(3.0, 0.0), badge.max - vec2(10.0, 0.0)),
                format!("P{}", loaded.number),
                11.0,
                ink,
            );
            let center = badge.right_center() - vec2(6.0, 0.0);
            match loaded.state {
                State::Playing => {
                    painter.add(Shape::convex_polygon(
                        vec![
                            center + vec2(-2.5, -4.0),
                            center + vec2(3.0, 0.0),
                            center + vec2(-2.5, 4.0),
                        ],
                        ink,
                        Stroke::NONE,
                    ));
                }
                State::Paused => {
                    for dx in [-2.0, 2.0] {
                        painter.line_segment(
                            [center + vec2(dx, -3.5), center + vec2(dx, 3.5)],
                            Stroke::new(1.5, ink),
                        );
                    }
                }
                State::Cued => {
                    painter.circle_stroke(center, 3.0, Stroke::new(1.5, ink));
                }
                State::Ended | State::Loaded => {
                    painter.rect_filled(Rect::from_center_size(center, vec2(5.0, 5.0)), 0, ink);
                }
            }
            ui.interact(
                badge.intersect(ui.clip_rect()),
                ui.scope_id()
                    .with(("on-deck", track.id, loaded.number, badge.top().to_bits())),
                Sense::hover(),
            )
            .on_hover_text(format!(
                "Player {} · {}",
                loaded.number,
                loaded.state.label()
            ));
            self.hit(
                &format!(
                    "On deck {} P{} {}",
                    track.id,
                    loaded.number,
                    loaded.state.label()
                ),
                badge,
            );
            rect.min.x += 37.0;
        }
        rect
    }
}
