// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Deserializer, Serialize};

pub fn null_vec<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Vec<T>, D::Error> {
    Ok(Option::<Vec<T>>::deserialize(d)?.unwrap_or_default())
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Track {
    pub id: u32,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub rating: u8,
    pub color_id: u8,
    pub date_added: String,
    pub bpm: f64,
    pub key: String,
    pub duration: u32,
    pub file_path: String,
    pub file_type: String,
    pub file_missing: bool,
    pub analysis_locked: bool,
    pub art_id: u32,
    pub art_checked: bool,
    #[serde(deserialize_with = "null_vec")]
    pub tags: Vec<String>,
}

impl Track {
    pub fn search_text(&self) -> String {
        format!(
            "{} {} {} {} {}",
            self.title,
            self.artist,
            self.album,
            self.genre,
            self.tags.join(" ")
        )
        .to_lowercase()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Playlist {
    pub id: u32,
    pub parent_id: u32,
    pub name: String,
    pub is_folder: bool,
    pub is_smart: bool,
    pub read_only: bool,
    pub rules: Option<serde_json::Value>,
    #[serde(deserialize_with = "null_vec")]
    pub track_ids: Vec<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Status {
    #[serde(deserialize_with = "null_vec")]
    pub peers: Vec<Peer>,
    #[serde(deserialize_with = "null_vec")]
    pub players: Vec<Player>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Peer {
    pub device_number: u8,
    pub name: String,
    pub device_type: String,
    pub ip: String,
    pub mac: String,
    pub mixer: Option<Mixer>,
}

impl Peer {
    pub fn same_device(&self, other: &Self) -> bool {
        self.device_number == other.device_number
            && self.name == other.name
            && self.device_type == other.device_type
            && self.ip == other.ip
            && self.mac.eq_ignore_ascii_case(&other.mac)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Mixer {
    pub channel_state_known: bool,
    pub channel_on_air: Vec<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Player {
    pub device_number: u8,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub track_id: u32,
    pub track_title: String,
    pub artist: String,
    pub bpm: f64,
    pub key: String,
    pub is_playing: bool,
    pub on_air: bool,
    #[serde(default)]
    pub is_master: bool,
    #[serde(default)]
    pub external: bool,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub play_state: u8,
    #[serde(default)]
    pub play_state_name: String,
    #[serde(default)]
    pub beat_in_bar: u8,
    #[serde(default)]
    pub pitch_pct: f64,
    #[serde(default)]
    pub is_sync: bool,
    #[serde(default)]
    pub beat_in_track: u32,
    #[serde(default)]
    pub beat_age_ms: u32,
    #[serde(default)]
    pub duration_ms: u32,
    #[serde(default)]
    pub artwork_url: String,
    #[serde(default)]
    pub analysis_url: String,
}

impl Player {
    /// Holding CUE advances audio even when the CDJ's separate playing flag is clear.
    pub fn is_forward_playback(&self) -> bool {
        (self.is_playing || self.play_state == 7) && self.play_state_name != "REVERSE"
    }

    pub fn needs_load_confirmation(&self) -> bool {
        self.is_playing || matches!(self.play_state, 3 | 4 | 7)
    }

    pub fn same_loaded_track(&self, other: &Self) -> bool {
        self.device_number == other.device_number
            && self.track_id == other.track_id
            && self.external == other.external
            && self.source == other.source
            && (!self.external || self.analysis_url == other.analysis_url)
            && (self.track_id != 0
                || (self.track_title == other.track_title && self.artist == other.artist))
    }
}

impl Status {
    pub fn load_error(&self, target: &Peer) -> Option<&'static str> {
        if target.device_type != "cdj" || !(1..=4).contains(&target.device_number) {
            return Some("Choose a CDJ player 1–4");
        }
        if self
            .peers
            .iter()
            .filter(|p| p.device_number == target.device_number)
            .count()
            != 1
            || !self.peers.iter().any(|p| p.same_device(target))
        {
            return Some("Player changed or disconnected");
        }
        if self
            .players
            .iter()
            .filter(|p| p.device_number == target.device_number)
            .count()
            != 1
        {
            return Some("Waiting for player status");
        }
        let player = self
            .players
            .iter()
            .find(|p| p.device_number == target.device_number)?;
        if !player.needs_load_confirmation()
            && !matches!(
                (player.play_state, player.play_state_name.as_str()),
                (0, "NO TRACK") | (5, "PAUSED") | (6, "CUED") | (0x11, "ENDED")
            )
        {
            return Some("Waiting for a known player state");
        }
        None
    }

    /// The API worker repeats this check against fresh status just before POST.
    /// Confirmation applies only to the track/source shown in the dialog.
    pub fn load_confirmation_error(
        &self,
        target: &Peer,
        confirmed: Option<&Player>,
    ) -> Option<&'static str> {
        if let Some(error) = self.load_error(target) {
            return Some(error);
        }
        let player = self
            .players
            .iter()
            .find(|p| p.device_number == target.device_number)?;
        if let Some(confirmed) = confirmed {
            if !player.same_loaded_track(confirmed) {
                return Some("Player's track changed; choose the player again");
            }
        } else if player.needs_load_confirmation() {
            return Some("Player is playing; choose the player again to confirm loading");
        }
        None
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Cue {
    pub number: u16,
    #[serde(rename = "type")]
    pub kind: String,
    pub time_ms: u32,
    pub loop_ms: i32,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub color_id: u32,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct WaveEntry {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub h: u8,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Waveform {
    pub track_id: u32,
    pub sample_rate: u32,
    #[serde(deserialize_with = "null_vec")]
    pub entries: Vec<WaveEntry>,
}

#[derive(Clone, Default)]
pub struct Detail {
    pub waveform: Waveform,
    pub bands: crate::waveform::Bands,
    pub beat_grid: crate::beat_grid::BeatGrid,
    pub phrases: crate::phrases::Sections,
    pub cues: Vec<Cue>,
    pub cues_ready: bool,
    pub warning: String,
}

pub fn time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u32;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_go_null_collections_but_requires_control_flags() {
        let s: Status = serde_json::from_str(r#"{"peers":null,"players":null}"#).unwrap();
        assert!(s.players.is_empty());
        assert!(serde_json::from_str::<Player>(r#"{"device_number":1}"#).is_err());
    }
    #[test]
    fn loads_idle_players_and_requires_confirmation_for_playing_players() {
        let mut s = crate::demo::status();
        let peer = s.peers[1].clone();
        assert_eq!(s.load_error(&peer), None);
        s.players[1].is_playing = true;
        assert_eq!(s.load_error(&peer), None);
        assert!(s.load_confirmation_error(&peer, None).is_some());
        let confirmed = s.players[1].clone();
        assert_eq!(s.load_confirmation_error(&peer, Some(&confirmed)), None);
        s.players[1].is_playing = false;
        s.players[1].on_air = true;
        assert_eq!(s.load_error(&peer), None);
        assert_eq!(s.load_confirmation_error(&peer, None), None);
        for state in [3, 4, 7] {
            s.players[1].play_state = state;
            assert!(s.load_confirmation_error(&peer, None).is_some());
        }
        s.players[1].play_state = 9;
        assert!(
            s.load_error(&peer).is_some(),
            "unknown/transitional states stay blocked"
        );
        s.players[1].on_air = false;
        s.peers[1].ip = "10.0.0.2".into();
        assert!(s.load_error(&peer).is_some());
        s.players.clear();
        assert!(s.load_error(&peer).is_some());
    }

    #[test]
    fn load_confirmation_is_bound_to_the_loaded_track_and_external_source() {
        let mut s = crate::demo::status();
        let peer = s.peers[0].clone();
        s.players[0].external = true;
        s.players[0].source = "USB".into();
        s.players[0].analysis_url = "/api/analysis/ext/2/3/1".into();
        let confirmed = s.players[0].clone();
        assert_eq!(s.load_confirmation_error(&peer, Some(&confirmed)), None);
        s.players[0].analysis_url = "/api/analysis/ext/3/3/1".into();
        assert!(s.load_confirmation_error(&peer, Some(&confirmed)).is_some());
        s.players[0] = confirmed.clone();
        s.players[0].track_id = 2;
        assert!(s.load_confirmation_error(&peer, Some(&confirmed)).is_some());
        s.players[0] = confirmed.clone();
        s.players[0].is_playing = false;
        s.players[0].play_state = 5;
        s.players[0].play_state_name = "PAUSED".into();
        assert_eq!(s.load_confirmation_error(&peer, Some(&confirmed)), None);
        assert_eq!(s.load_confirmation_error(&peer, None), None);
    }
}
