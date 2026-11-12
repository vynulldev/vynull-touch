// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::{Player, Status};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Reference {
    #[default]
    Master,
    Player(u8),
    Off,
}

impl Reference {
    pub fn player(self, status: &Status, live: bool) -> Option<&Player> {
        if !live || self == Self::Off {
            return None;
        }
        let mut players = status.players.iter().filter(|p| match self {
            Self::Master => p.is_master,
            Self::Player(number) => p.device_number == number,
            Self::Off => false,
        });
        let player = players.next()?;
        // An ambiguous master or device number must not silently choose a deck.
        if players.next().is_some()
            || !(1..=4).contains(&player.device_number)
            || status
                .players
                .iter()
                .filter(|p| p.device_number == player.device_number)
                .count()
                != 1
        {
            return None;
        }
        let mut peers = status
            .peers
            .iter()
            .filter(|p| p.device_number == player.device_number);
        let peer = peers.next()?;
        if peers.next().is_some()
            || peer.device_type != "cdj"
            || (player.track_id == 0 && player.track_title.trim().is_empty())
            || matches!(
                player.play_state_name.as_str(),
                "NO TRACK" | "LOADING" | "EJECTING"
            )
        {
            return None;
        }
        Some(player)
    }
}
