// SPDX-License-Identifier: GPL-3.0-or-later
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const DEFAULT_ZOOM: f64 = 8.0;
pub const BEAT_JUMP_SIZES: [u8; 6] = [1, 2, 4, 8, 16, 32];

#[derive(Deserialize, Serialize)]
#[serde(default)]
pub struct Settings {
    pub server: Option<String>,
    pub startup_page: StartupPage,
    pub waveform_zoom: f64,
    pub beat_jump_beats: u8,
    pub inspect_quantize: bool,
    pub waveform_style: crate::waveform::Style,
    pub waveform_overview: crate::waveform::Overview,
    pub audio_output: Option<crate::preview::OutputDevice>,
    pub audio_volume: f32,
    pub players: Players,
    pub library_order: crate::library::Order,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupPage {
    Players,
    Playlists,
    #[default]
    #[serde(other)]
    Collection,
}

impl StartupPage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Players => "Players",
            Self::Collection => "Collection",
            Self::Playlists => "Playlists",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct Players {
    pub pair: [u8; 2],
    pub compare: bool,
    pub waveform_span: f64,
}

impl Default for Players {
    fn default() -> Self {
        Self {
            pair: [1, 2],
            compare: true,
            waveform_span: 16.0,
        }
    }
}

fn valid_pair(pair: [u8; 2]) -> bool {
    pair[0] > 0 && pair[1] > 0 && pair[0] != pair[1]
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            server: None,
            startup_page: StartupPage::default(),
            waveform_zoom: DEFAULT_ZOOM,
            beat_jump_beats: 4,
            inspect_quantize: false,
            waveform_style: Default::default(),
            waveform_overview: Default::default(),
            audio_output: None,
            audio_volume: 0.5,
            players: Players::default(),
            library_order: Default::default(),
        }
    }
}

pub fn path() -> Option<PathBuf> {
    config_path(
        std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
        std::env::var_os("HOME").map(PathBuf::from),
    )
}

fn config_path(xdg: Option<PathBuf>, home: Option<PathBuf>) -> Option<PathBuf> {
    xdg.filter(|p| p.is_absolute())
        .or_else(|| home.filter(|p| p.is_absolute()).map(|p| p.join(".config")))
        .map(|p| p.join("vynull-touch/settings.json"))
}

pub fn load(path: Option<&Path>) -> Settings {
    let mut settings = path
        .and_then(|p| {
            fs::read(p)
                .or_else(|error| {
                    // Only a missing new file permits the legacy fallback. An
                    // invalid or unreadable new file must not revive old settings.
                    if error.kind() == io::ErrorKind::NotFound
                        && p.file_name().is_some_and(|name| name == "settings.json")
                        && let Some(parent) = p.parent()
                        && parent
                            .file_name()
                            .is_some_and(|name| name == "vynull-touch")
                        && let Some(root) = parent.parent()
                    {
                        fs::read(root.join("vynull-pi/settings.json"))
                    } else {
                        Err(error)
                    }
                })
                .ok()
        })
        .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        .unwrap_or_default();
    if settings
        .server
        .as_ref()
        .is_some_and(|server| crate::api::validate_origin(server).is_err())
    {
        settings.server = None;
    }
    if !(1.0..=256.0).contains(&settings.waveform_zoom) {
        settings.waveform_zoom = DEFAULT_ZOOM;
    }
    if !BEAT_JUMP_SIZES.contains(&settings.beat_jump_beats) {
        settings.beat_jump_beats = Settings::default().beat_jump_beats;
    }
    if !(0.0..=1.0).contains(&settings.audio_volume) {
        settings.audio_volume = 0.5;
    }
    if !valid_pair(settings.players.pair) {
        settings.players.pair = Players::default().pair;
    }
    if !(4.0..=64.0).contains(&settings.players.waveform_span) {
        settings.players.waveform_span = Players::default().waveform_span;
    }
    settings
}

#[cfg(test)]
pub fn load_zoom(path: Option<&Path>) -> f64 {
    load(path).waveform_zoom
}

pub fn save_zoom(path: &Path, zoom: f64) -> io::Result<()> {
    if !(1.0..=256.0).contains(&zoom) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid waveform zoom",
        ));
    }
    let mut settings = load(Some(path));
    settings.waveform_zoom = zoom;
    save(path, &settings)
}

pub fn save_waveform_style(path: &Path, style: crate::waveform::Style) -> io::Result<()> {
    let mut settings = load(Some(path));
    settings.waveform_style = style;
    save(path, &settings)
}

pub fn save_beat_jump(path: &Path, beats: u8) -> io::Result<()> {
    if !BEAT_JUMP_SIZES.contains(&beats) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid beat jump size",
        ));
    }
    let mut settings = load(Some(path));
    settings.beat_jump_beats = beats;
    save(path, &settings)
}

pub fn save_inspect_quantize(path: &Path, enabled: bool) -> io::Result<()> {
    let mut settings = load(Some(path));
    settings.inspect_quantize = enabled;
    save(path, &settings)
}

pub fn save_waveform_overview(path: &Path, overview: crate::waveform::Overview) -> io::Result<()> {
    let mut settings = load(Some(path));
    settings.waveform_overview = overview;
    save(path, &settings)
}

pub fn save_audio(
    path: &Path,
    output: Option<crate::preview::OutputDevice>,
    volume: f32,
) -> io::Result<()> {
    if !(0.0..=1.0).contains(&volume) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid preview volume",
        ));
    }
    let mut settings = load(Some(path));
    settings.audio_output = output;
    settings.audio_volume = volume;
    save(path, &settings)
}

pub fn save_players(path: &Path, players: Players) -> io::Result<()> {
    if !valid_pair(players.pair) || !(4.0..=64.0).contains(&players.waveform_span) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid player view settings",
        ));
    }
    let mut settings = load(Some(path));
    settings.players = players;
    save(path, &settings)
}

pub fn save_order(path: &Path, order: crate::library::Order) -> io::Result<()> {
    let mut settings = load(Some(path));
    settings.library_order = order;
    save(path, &settings)
}

pub fn save_server(path: &Path, server: &str) -> io::Result<()> {
    crate::api::validate_origin(server)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    let mut settings = load(Some(path));
    settings.server = Some(server.to_owned());
    save(path, &settings)
}

pub fn save_startup_page(path: &Path, page: StartupPage) -> io::Result<()> {
    let mut settings = load(Some(path));
    settings.startup_page = page;
    save(path, &settings)
}

fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(settings)?;
    // Each click is saved immediately. Rename prevents an interrupted write
    // from replacing the last valid settings with a partial JSON file.
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn waveform_styles_persist_and_unknown_names_preserve_other_preferences() {
        use crate::waveform::{Overview, Style};
        let root = std::env::temp_dir().join(format!("vynull-styles-{}", std::process::id()));
        let path = root.join("settings.json");
        save_zoom(&path, 64.0).unwrap();
        assert_eq!(load(Some(&path)).beat_jump_beats, 4);
        assert!(!load(Some(&path)).inspect_quantize);
        save_inspect_quantize(&path, true).unwrap();
        save_beat_jump(&path, 16).unwrap();
        assert!(save_beat_jump(&path, 3).is_err());
        assert_eq!(load(Some(&path)).waveform_overview, Overview::Full);
        save_waveform_overview(&path, Overview::Half).unwrap();
        for style in Style::ALL {
            save_waveform_style(&path, style).unwrap();
            save_audio(&path, None, 0.25).unwrap();
            save_players(
                &path,
                Players {
                    compare: false,
                    ..Default::default()
                },
            )
            .unwrap();
            let saved = load(Some(&path));
            assert_eq!(saved.waveform_style, style);
            assert_eq!(saved.waveform_overview, Overview::Half);
            assert_eq!(saved.waveform_zoom, 64.0);
            assert_eq!(saved.beat_jump_beats, 16);
            assert!(saved.inspect_quantize);
            assert!(!saved.players.compare);
        }
        fs::write(
            &path,
            r#"{"waveform_style":"future","waveform_overview":"future","waveform_zoom":32,"audio_volume":0.3,"beat_jump_beats":3}"#,
        )
        .unwrap();
        let saved = load(Some(&path));
        assert_eq!(saved.waveform_style, Style::Auto);
        assert_eq!(saved.waveform_overview, Overview::Full);
        assert_eq!(saved.waveform_zoom, 32.0);
        assert_eq!(saved.beat_jump_beats, 4);
        assert_eq!(saved.audio_volume, 0.3);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn connection_preferences_validate_and_survive_unrelated_saves() {
        let root =
            std::env::temp_dir().join(format!("vynull-startup-settings-{}", std::process::id()));
        let path = root.join("settings.json");
        save_server(&path, "http://pi.local:9443").unwrap();
        save_startup_page(&path, StartupPage::Players).unwrap();
        save_zoom(&path, 64.0).unwrap();
        save_audio(&path, None, 0.25).unwrap();
        save_players(
            &path,
            Players {
                pair: [3, 1],
                ..Default::default()
            },
        )
        .unwrap();
        save_order(&path, crate::library::Order::default()).unwrap();
        let saved = load(Some(&path));
        assert_eq!(saved.server.as_deref(), Some("http://pi.local:9443"));
        assert_eq!(saved.startup_page, StartupPage::Players);
        for invalid in [
            "",
            "file:///tmp/test",
            "http://user:password@host",
            "http://host/api",
        ] {
            assert!(save_server(&path, invalid).is_err());
            assert_eq!(load(Some(&path)).server, saved.server);
        }
        fs::write(&path, r#"{"server":"file:///tmp/test","startup_page":"future_tab","waveform_zoom":64,"audio_volume":0.25}"#).unwrap();
        let recovered = load(Some(&path));
        assert!(recovered.server.is_none());
        assert_eq!(recovered.startup_page, StartupPage::Collection);
        assert_eq!(recovered.waveform_zoom, 64.0);
        assert_eq!(recovered.audio_volume, 0.25);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_preserves_legacy_preferences_and_new_settings_take_precedence() {
        let root = std::env::temp_dir().join(format!(
            "vynull-touch-rename-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let legacy = root.join("vynull-pi/settings.json");
        let current = config_path(Some(root.clone()), None).unwrap();
        let output = crate::preview::OutputDevice {
            id: "alsa:saved".into(),
            name: "Saved DAC".into(),
            host: "ALSA".into(),
        };
        let players = Players {
            pair: [3, 1],
            compare: true,
            waveform_span: 32.0,
        };
        save_zoom(&legacy, 16.0).unwrap();
        save_audio(&legacy, Some(output.clone()), 0.3).unwrap();
        save_players(&legacy, players).unwrap();
        let original = fs::read(&legacy).unwrap();

        let restored = load(Some(&current));
        assert_eq!(restored.waveform_zoom, 16.0);
        assert_eq!(restored.audio_output, Some(output.clone()));
        assert_eq!(restored.audio_volume, 0.3);
        assert_eq!(restored.players, players);
        assert!(!current.exists());
        // An unrelated settings file must not inherit the old app's preferences.
        assert_eq!(load_zoom(Some(&root.join("other/settings.json"))), 8.0);

        save_zoom(&current, 64.0).unwrap();
        let migrated = load(Some(&current));
        assert_eq!(migrated.waveform_zoom, 64.0);
        assert_eq!(migrated.audio_output, Some(output));
        assert_eq!(migrated.audio_volume, 0.3);
        assert_eq!(migrated.players, players);
        assert_eq!(fs::read(&legacy).unwrap(), original);

        fs::write(&current, b"broken JSON").unwrap();
        assert_eq!(load_zoom(Some(&current)), 8.0);
        fs::remove_file(&current).unwrap();
        fs::create_dir(&current).unwrap();
        assert_eq!(load_zoom(Some(&current)), 8.0);
        assert_eq!(fs::read(&legacy).unwrap(), original);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn player_settings_accept_legacy_files_and_validate_without_losing_other_preferences() {
        let root =
            std::env::temp_dir().join(format!("vynull-player-settings-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        fs::write(&path, r#"{"waveform_zoom":32,"audio_volume":0.3}"#).unwrap();
        assert_eq!(load(Some(&path)).players, Players::default());
        let players = Players {
            pair: [4, 2],
            compare: true,
            waveform_span: 4.0,
        };
        save_players(&path, players).unwrap();
        assert_eq!(load(Some(&path)).waveform_zoom, 32.0);
        assert_eq!(load(Some(&path)).audio_volume, 0.3);
        for invalid in [
            Players {
                pair: [0, 2],
                ..players
            },
            Players {
                pair: [1, 0],
                ..players
            },
            Players {
                pair: [3, 3],
                ..players
            },
            Players {
                waveform_span: 0.0,
                ..players
            },
            Players {
                waveform_span: 128.0,
                ..players
            },
            Players {
                waveform_span: f64::NAN,
                ..players
            },
            Players {
                waveform_span: f64::INFINITY,
                ..players
            },
        ] {
            assert!(save_players(&path, invalid).is_err());
            assert_eq!(load(Some(&path)).players, players);
        }
        fs::write(&path, r#"{"waveform_zoom":32,"audio_volume":0.3,"players":{"pair":[3,3],"compare":true,"waveform_span":128}}"#).unwrap();
        let recovered = load(Some(&path));
        assert_eq!(recovered.waveform_zoom, 32.0);
        assert_eq!(recovered.audio_volume, 0.3);
        assert_eq!(
            recovered.players,
            Players {
                compare: true,
                ..Players::default()
            }
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn persists_zoom_and_fit_and_recovers_from_invalid_settings() {
        let root = std::env::temp_dir().join(format!(
            "vynull-touch-settings-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("settings.json");
        assert_eq!(load_zoom(None), 8.0);
        assert_eq!(load_zoom(Some(&path)), 8.0);
        for zoom in [16.0, 256.0, 1.0] {
            save_zoom(&path, zoom).unwrap();
            assert_eq!(load_zoom(Some(&path)), zoom);
        }
        for zoom in [0.0, -1.0, 512.0, f64::NAN, f64::INFINITY] {
            assert!(save_zoom(&path, zoom).is_err());
            assert_eq!(
                load_zoom(Some(&path)),
                1.0,
                "invalid input must preserve the last saved setting"
            );
        }
        for json in [
            "broken JSON",
            "{}",
            r#"{"waveform_zoom":0}"#,
            r#"{"waveform_zoom":512}"#,
            r#"{"waveform_zoom":null}"#,
        ] {
            fs::write(&path, json).unwrap();
            assert_eq!(load_zoom(Some(&path)), 8.0);
        }
        save_zoom(&path, 32.0).unwrap();
        assert_eq!(load_zoom(Some(&path)), 32.0);
        let output = crate::preview::OutputDevice {
            id: "alsa:test".into(),
            name: "Test DAC".into(),
            host: "ALSA".into(),
        };
        save_audio(&path, Some(output.clone()), 0.3).unwrap();
        assert_eq!(load_zoom(Some(&path)), 32.0);
        save_zoom(&path, 64.0).unwrap();
        assert_eq!(load(Some(&path)).audio_output, Some(output));
        assert_eq!(load(Some(&path)).audio_volume, 0.3);
        assert!(save_audio(&path, None, f32::NAN).is_err());
        // A non-directory parent must produce an error, without damaging it.
        assert!(save_zoom(&path.join("settings.json"), 64.0).is_err());
        assert_eq!(load_zoom(Some(&path)), 64.0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uses_xdg_config_or_home_and_ignores_relative_paths() {
        let home = Some(PathBuf::from("/home/listener"));
        assert_eq!(
            config_path(Some("/tmp/config".into()), home.clone()),
            Some("/tmp/config/vynull-touch/settings.json".into())
        );
        for xdg in [None, Some("".into()), Some("relative".into())] {
            assert_eq!(
                config_path(xdg, home.clone()),
                Some("/home/listener/.config/vynull-touch/settings.json".into())
            );
        }
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn library_sort_survives_other_settings_writes_and_unknown_fields_keep_preferences() {
        let root = std::env::temp_dir().join(format!("vynull-touch-sort-{}", std::process::id()));
        let path = root.join("settings.json");
        save_zoom(&path, 32.0).unwrap();
        let order = crate::library::Order {
            field: crate::library::Sort::Added,
            descending: true,
        };
        save_order(&path, order).unwrap();
        save_audio(&path, None, 0.25).unwrap();
        save_players(
            &path,
            Players {
                pair: [4, 2],
                ..Default::default()
            },
        )
        .unwrap();
        let saved = load(Some(&path));
        assert_eq!(saved.library_order, order);
        assert_eq!(saved.waveform_zoom, 32.0);
        assert_eq!(saved.audio_volume, 0.25);
        assert_eq!(saved.players.pair, [4, 2]);
        fs::write(
            &path,
            r#"{"waveform_zoom":64,"library_order":{"field":"FutureField","descending":true}}"#,
        )
        .unwrap();
        let saved = load(Some(&path));
        assert_eq!(saved.waveform_zoom, 64.0);
        assert_eq!(saved.library_order.field, crate::library::Sort::Original);
        fs::remove_dir_all(root).unwrap();
    }
}
