// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::*;

pub fn tracks() -> Vec<Track> {
    let titles = [
        "After the Rain",
        "Night Transit",
        "Soft Signal",
        "Open Water",
        "Low Orbit",
        "Glasshouse",
        "Parallel Lines",
        "Daybreak",
        "Slow Motion",
        "Outer Reach",
        "Still Here",
        "First Light",
    ];
    let artists = ["Mira Vale", "North Circuit", "Aster Fields", "Niko Reed"];
    (0..96)
        .map(|i| Track {
            id: i + 1,
            title: if i < 12 {
                titles[i as usize % 12].into()
            } else {
                format!("{} / {:02}", titles[i as usize % 12], i / 12 + 1)
            },
            artist: artists[i as usize % 4].into(),
            album: "Vynull demo collection".into(),
            genre: if i % 3 == 0 { "Deep House" } else { "Minimal" }.into(),
            rating: (i % 6) as u8,
            color_id: (i % 9) as u8,
            date_added: format!("2026-10-{:02}", i % 28 + 1),
            bpm: 120.0 + f64::from(i % 12),
            key: format!("{}{}", i % 12 + 1, if i % 3 == 0 { "B" } else { "A" }),
            duration: 284 + i * 3,
            file_type: "FLAC".into(),
            file_path: format!("demo://{}", i + 1),
            tags: vec!["Demo".into()],
            ..Default::default()
        })
        .collect()
}

/// Small fictional previews; demo browsing needs no full-track analysis or I/O.
pub fn library_waveform(id: u32) -> Vec<crate::waveform::BandEntry> {
    (0..160)
        .map(|i| {
            let x = i as f32 / 160.0;
            let seed = id as f32 * 0.73;
            let phrase = ((x * 19.0 + seed).sin().abs() * 0.65 + 0.35)
                * (x * 12.0).min(1.0)
                * ((1.0 - x) * 14.0).min(1.0);
            let transient = (x * 297.0 + seed).sin().abs();
            crate::waveform::BandEntry {
                bass: ((100.0 + 80.0 * transient) * phrase) as u8,
                mid: ((45.0 + 90.0 * (x * 41.0 + seed).sin().abs()) * phrase) as u8,
                high: ((8.0 + 80.0 * transient.powf(5.0)) * phrase) as u8,
            }
        })
        .collect()
}

pub fn playlists() -> Vec<Playlist> {
    [
        "Warm-up selections",
        "After hours",
        "Deep & rolling",
        "Sunday sessions",
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| Playlist {
        id: i as u32 + 1,
        name: (*name).into(),
        track_ids: (1..=96).filter(|n| (*n as usize - 1) % 4 == i).collect(),
        ..Default::default()
    })
    .collect()
}

pub fn status() -> Status {
    Status {
        peers: (1..=2)
            .map(|n| Peer {
                device_number: n,
                name: "CDJ-3000".into(),
                device_type: "cdj".into(),
                ip: format!("169.254.1.{n}"),
                mac: format!("02:00:00:00:00:{n:02x}"),
                ..Default::default()
            })
            .collect(),
        players: vec![
            Player {
                device_number: 1,
                name: "CDJ-3000".into(),
                track_id: 1,
                track_title: "After the Rain".into(),
                artist: "Mira Vale".into(),
                bpm: 120.0,
                key: "1B".into(),
                is_playing: true,
                is_master: true,
                on_air: true,
                play_state: 3,
                play_state_name: "PLAYING".into(),
                beat_in_bar: 2,
                beat_in_track: 66,
                beat_age_ms: 180,
                duration_ms: 284_000,
                pitch_pct: 1.25,
                ..Default::default()
            },
            Player {
                device_number: 2,
                name: "CDJ-3000".into(),
                play_state_name: "NO TRACK".into(),
                ..Default::default()
            },
        ],
    }
}

pub fn cues() -> Vec<Cue> {
    [0, 32_000, 96_000, 192_000]
        .iter()
        .enumerate()
        .map(|(i, &time_ms)| Cue {
            number: i as u16 + 1,
            kind: "cue".into(),
            time_ms,
            loop_ms: -1,
            label: ["Start", "Build", "Drop", "Outro"][i].into(),
            color_id: [0x16, 0x24, 0x2a, 0x01][i],
        })
        .collect()
}

pub fn detail(track: &Track) -> Detail {
    let rate = 150;
    let entries = (0..track.duration * rate)
        .map(|i| {
            let t = i as f32 * 12.0 / rate as f32;
            let envelope = (t * 0.007).sin().abs() * 0.45 + 0.4;
            let beat = (t * 1.51).sin().abs() * 0.6 + (t * 0.31).sin().abs() * 0.4;
            WaveEntry {
                r: ((t * 0.015).sin().abs() * 5.0) as u8 + 1,
                g: 3,
                b: 7,
                h: (beat * envelope * 28.0 + 2.0) as u8,
            }
        })
        .collect();
    Detail {
        phrases: phrases(track),
        bands: crate::waveform::Bands {
            track_id: track.id,
            style: "3band".into(),
            sample_rate: rate,
            entries: (0..track.duration * rate)
                .map(|i| {
                    let t = i as f32 / rate as f32;
                    let beat = (t * track.bpm as f32 / 60.0).fract();
                    let envelope = (t * 0.084).sin().abs() * 0.7 + 0.2;
                    crate::waveform::BandEntry {
                        bass: (((1.0 - beat).powf(3.0) * 205.0 + 15.0) * envelope) as u8,
                        mid: (((t * 19.0).sin().abs() * 75.0 + 55.0)
                            * envelope
                            * (0.4 + 0.6 * (t * 0.047).sin().abs()))
                            as u8,
                        high: (((t * 39.0).sin().abs() * 35.0 + 8.0)
                            * envelope
                            * (0.3 + 0.7 * (t * 0.061).sin().abs()))
                            as u8,
                    }
                })
                .collect(),
        },
        beat_grid: crate::beat_grid::BeatGrid::new(
            crate::beat_grid::TrackAnalysis {
                track_id: track.id,
                beats: (0..(f64::from(track.duration) * track.bpm / 60.0).ceil() as u32)
                    .map(|i| crate::beat_grid::Beat {
                        time_ms: f64::from(i) * 60_000.0 / track.bpm,
                        beat_in_bar: (i % 4 + 1) as u8,
                    })
                    .collect(),
            },
            track.id,
        )
        .expect("valid demo beat grid"),
        waveform: Waveform {
            track_id: track.id,
            sample_rate: rate,
            entries,
        },
        cues: cues(),
        warning: String::new(),
        cues_ready: true,
    }
}

fn phrases(track: &Track) -> crate::phrases::Sections {
    let duration_ms = f64::from(track.duration) * 1000.0;
    let bar_ms = 240_000.0 / track.bpm;
    let boundaries = [0.0, 0.12, 0.24, 0.45, 0.56, 0.68, 0.9, 1.0].map(|fraction| {
        if fraction == 1.0 {
            duration_ms as u32
        } else {
            ((fraction * duration_ms / bar_ms).floor() * bar_ms) as u32
        }
    });
    crate::phrases::Sections::new(
        [1, 2, 5, 3, 2, 5, 6]
            .into_iter()
            .enumerate()
            .map(|(i, kind)| crate::phrases::Phrase {
                kind,
                name: String::new(),
                start_ms: boundaries[i],
                end_ms: boundaries[i + 1],
            })
            .collect(),
    )
    .expect("valid demo phrases")
}
