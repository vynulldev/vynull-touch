// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use crate::{
    artwork,
    beat_grid::Beat,
    decks::{Assets, Key, summarize},
};
use eframe::egui::ColorImage;

#[derive(Deserialize)]
struct External {
    track_id: u32,
    #[serde(default)]
    duration_ms: u32,
    #[serde(default)]
    bpm: f64,
    #[serde(default)]
    sample_rate: u32,
    #[serde(default, deserialize_with = "null_vec")]
    waveform: Vec<WaveEntry>,
    #[serde(default, deserialize_with = "null_vec")]
    beats: Vec<f64>,
    #[serde(default, deserialize_with = "null_vec")]
    cues: Vec<Cue>,
}
#[derive(Deserialize)]
struct Local {
    track_id: u32,
    #[serde(default)]
    bpm: f64,
    #[serde(default, deserialize_with = "null_vec")]
    beats: Vec<Beat>,
}

pub(crate) fn fetch(
    origin: &str,
    session: Option<String>,
    key: &Key,
) -> (Assets, Option<ColorImage>) {
    let mut assets = Assets::default();
    let result = (|| -> Result<(), String> {
        if key.external && !key.valid_external() {
            return Err("External track analysis is unavailable".into());
        }
        let mut api = Api::new(origin)?;
        api.session = session.clone();
        if key.external {
            let data: External = api.json(Method::GET, &key.analysis, None, 15)?;
            if data.track_id != key.track {
                return Err("External waveform identity changed".into());
            }
            assets.duration = if data.duration_ms > 0 {
                f64::from(data.duration_ms) / 1000.0
            } else if data.sample_rate > 0 {
                data.waveform.len() as f64 / f64::from(data.sample_rate)
            } else {
                0.0
            };
            assets.bpm = data.bpm;
            assets.overview = summarize(&data.waveform);
            if data.sample_rate > 0 {
                assets.sample_rate = data.sample_rate;
                assets.detail = data.waveform.into();
            }
            assets.cues = data.cues;
            if data.beats.iter().all(|b| b.is_finite())
                && data.beats.windows(2).all(|w| w[0] < w[1])
            {
                assets.beats = data.beats.into_iter().map(|b| b / 1000.0).collect();
            }
        } else {
            let wave: Result<Waveform, String> = api.json(
                Method::GET,
                &format!(
                    "api/analysis/waveform/{}?type=detail&style=spectral",
                    key.track
                ),
                None,
                15,
            );
            match wave {
                Ok(wave) if wave.track_id == key.track && wave.sample_rate > 0 => {
                    assets.duration = wave.entries.len() as f64 / f64::from(wave.sample_rate);
                    assets.overview = summarize(&wave.entries);
                    assets.sample_rate = wave.sample_rate;
                    assets.detail = wave.entries.into();
                }
                Ok(_) => assets.warning = "Waveform identity or sample rate is invalid".into(),
                Err(e) => assets.warning = e,
            }
            let analysis: Local = api.json(Method::GET, &key.analysis, None, 10)?;
            assets.grid = Arc::new(BeatGrid::new(
                TrackAnalysis {
                    track_id: analysis.track_id,
                    beats: analysis.beats.clone(),
                },
                key.track,
            )?);
            assets.bpm = analysis.bpm;
            assets.beats = analysis.beats.into_iter().map(|b| b.seconds()).collect();
            match api.list(&format!("api/tracks/{}/cues", key.track)) {
                Ok(cues) => assets.cues = cues,
                Err(e) => assets.warning = e,
            }
            let bands = api.waveform_bands(key.track)?;
            assets.band_overview = crate::waveform::summarize_bands(&bands.entries);
            assets.bands = Arc::new(bands);
            match api.phrase_sections(key.track) {
                Ok(phrases) => assets.phrases = Arc::new(phrases),
                Err(e) if e.starts_with("Library changed") => return Err(e),
                Err(e) => assets
                    .warning
                    .push_str(&format!(" Phrase sections unavailable: {e}")),
            }
        }
        Ok(())
    })();
    if let Err(e) = result {
        if e.starts_with("Library changed") {
            assets = Assets::default();
        }
        assets.warning = e;
    }
    // Only canonical library URLs or the exact source tuple are fetched.
    let image = if !key.art.is_empty() && (!key.external || key.valid_external()) {
        artwork::Fetcher::new(origin, session)
            .and_then(|f| f.fetch_path(&key.art))
            .ok()
            .flatten()
    } else {
        None
    };
    (assets, image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};

    #[test]
    fn optional_player_phrases_keep_analysis_but_session_changes_discard_the_snapshot() {
        for (status, session, body, warning) in [
            ("404 Not Found", "one", "{}", false),
            ("200 OK", "one", "null", false),
            ("503 Service Unavailable", "one", "{}", true),
            ("200 OK", "one", "{}", true),
            ("200 OK", "two", "[]", true),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                for (path, status, session, body) in [
                    (
                        "/api/analysis/waveform/7?type=detail&style=spectral",
                        "200 OK",
                        "one",
                        r#"{"track_id":7,"sample_rate":150,"entries":[{"r":1,"g":2,"b":3,"h":20}]}"#,
                    ),
                    (
                        "/api/analysis/7",
                        "200 OK",
                        "one",
                        r#"{"track_id":7,"bpm":120,"beats":[{"time_ms":0,"beat_in_bar":1},{"time_ms":500,"beat_in_bar":2}]}"#,
                    ),
                    ("/api/tracks/7/cues", "200 OK", "one", "[]"),
                    (
                        "/api/analysis/waveform/7?type=detail&style=3band",
                        "404 Not Found",
                        "one",
                        "{}",
                    ),
                    ("/api/tracks/7/phrases", status, session, body),
                ] {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut socket = loop {
                        match listener.accept() {
                            Ok((socket, _)) => break socket,
                            Err(e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    && Instant::now() < deadline =>
                            {
                                thread::sleep(Duration::from_millis(5))
                            }
                            Err(e) => panic!("waiting for {path}: {e}"),
                        }
                    };
                    socket
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut bytes = vec![];
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        socket.read_exact(&mut byte).unwrap();
                        bytes.push(byte[0]);
                    }
                    let request = String::from_utf8(bytes).unwrap().to_lowercase();
                    assert!(request.starts_with(&format!("get {path} ")));
                    assert!(request.contains("x-vynull-library-session: one"));
                    write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            let mut key = Key::new(
                None,
                &Player {
                    track_id: 7,
                    ..Default::default()
                },
            )
            .unwrap();
            key.art.clear();
            let (assets, _) = fetch(&origin, Some("one".into()), &key);
            server.join().unwrap();
            assert!(assets.phrases.is_empty());
            assert_eq!(!assets.warning.is_empty(), warning);
            assert_eq!(assets.detail.is_empty(), session != "one");
            assert_eq!(assets.grid.is_empty(), session != "one");
            if session != "one" {
                assert!(assets.warning.starts_with("Library changed"));
            }
        }
    }
    #[test]
    fn local_detail_keeps_original_samples_and_saved_bar_phase() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            for (path, body, status) in [
                (
                    "/api/analysis/waveform/7?type=detail&style=spectral",
                    r#"{"track_id":7,"sample_rate":150,"entries":[{"r":1,"g":2,"b":3,"h":2},{"r":4,"g":5,"b":6,"h":31},{"r":1,"g":2,"b":3,"h":4}]}"#,
                    "200 OK",
                ),
                (
                    "/api/analysis/7",
                    r#"{"track_id":7,"bpm":120,"beats":[{"time_ms":0,"beat_in_bar":3},{"time_ms":6.5,"beat_in_bar":4},{"time_ms":13.1,"beat_in_bar":1}]}"#,
                    "200 OK",
                ),
                ("/api/tracks/7/cues", "[]", "200 OK"),
                (
                    "/api/analysis/waveform/7?type=detail&style=3band",
                    r#"{"track_id":7,"style":"3band","sample_rate":150,"entries":[{"bass":255,"mid":128,"high":64}]}"#,
                    "200 OK",
                ),
                (
                    "/api/tracks/7/phrases",
                    r#"[{"kind":1,"name":"INTRO","start_ms":0,"end_ms":13},{"kind":5,"name":"CHORUS","start_ms":13,"end_ms":20}]"#,
                    "200 OK",
                ),
                ("/api/artwork/7", "{}", "404 Not Found"),
            ] {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut bytes = vec![];
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                }
                let request = String::from_utf8(bytes).unwrap();
                assert_eq!(request.split_whitespace().nth(1).unwrap(), path);
                assert!(
                    request
                        .to_lowercase()
                        .contains("x-vynull-library-session: one")
                );
                write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: one\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        let key = Key::new(
            None,
            &Player {
                track_id: 7,
                device_number: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let (assets, _) = fetch(&origin, Some("one".into()), &key);
        server.join().unwrap();
        assert!(assets.warning.is_empty(), "{}", assets.warning);
        assert_eq!(assets.sample_rate, 150);
        assert_eq!(assets.bands.entries[0].bass, 255);
        assert_eq!(assets.band_overview[0].mid, 128);
        assert_eq!(assets.phrases.at(0.013).0.unwrap().label(), "CHORUS");
        assert_eq!(
            assets.detail.iter().map(|e| e.h).collect::<Vec<_>>(),
            [2, 31, 4]
        );
        assert_eq!(assets.beats.len(), 3);
        for (actual, expected) in assets.beats.iter().zip([0.0, 0.0065, 0.0131]) {
            assert!((actual - expected).abs() < 1e-12);
        }
        assert_eq!(
            assets
                .grid
                .visible(0.0, 1.0, 20)
                .map(|b| b.beat_in_bar)
                .collect::<Vec<_>>(),
            [3, 4, 1]
        );
    }
    #[test]
    fn external_assets_use_the_source_tuple_and_keep_fractional_beats() {
        for (session, track_id, valid) in [("one", 7, true), ("two", 7, false), ("one", 8, false)] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let origin = format!("http://{}", listener.local_addr().unwrap());
            let server = thread::spawn(move || {
                let mut paths = vec![];
                for artwork in [false, true] {
                    let (mut socket, _) = listener.accept().unwrap();
                    socket
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut bytes = vec![];
                    let mut b = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        socket.read_exact(&mut b).unwrap();
                        bytes.push(b[0]);
                    }
                    let request = String::from_utf8(bytes).unwrap();
                    assert!(
                        request
                            .to_lowercase()
                            .contains("x-vynull-library-session: one")
                    );
                    paths.push(request.split_whitespace().nth(1).unwrap().to_owned());
                    let body = if artwork {
                        "{}".into()
                    } else {
                        format!(
                            r#"{{"track_id":{track_id},"duration_ms":120000,"bpm":123.4,"sample_rate":150,"waveform":[{{"r":1,"g":2,"b":3,"h":20}}],"beats":[125.5,611.75],"cues":null}}"#
                        )
                    };
                    let status = if artwork { "404 Not Found" } else { "200 OK" };
                    write!(socket,"HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
                }
                paths
            });
            let key = Key::new(
                None,
                &Player {
                    device_number: 1,
                    track_id: 7,
                    external: true,
                    analysis_url: "/api/analysis/ext/2/3/7".into(),
                    artwork_url: "/api/artwork/ext/2/3/7".into(),
                    ..Default::default()
                },
            )
            .unwrap();
            let (assets, image) = fetch(&origin, Some("one".into()), &key);
            assert!(image.is_none());
            if valid {
                assert_eq!(assets.duration, 120.0);
                assert_eq!(assets.beats, [0.1255, 0.61175]);
                assert_eq!(assets.overview.len(), 1);
                assert_eq!(assets.detail.len(), 1);
                assert_eq!(assets.sample_rate, 150);
                assert!(assets.grid.is_empty(), "external response has no bar phase");
                assert!(
                    assets.phrases.is_empty(),
                    "external IDs must not read library phrases"
                );
                assert!(assets.warning.is_empty());
            } else {
                assert!(assets.overview.is_empty());
                assert!(!assets.warning.is_empty());
            }
            assert_eq!(
                server.join().unwrap(),
                ["/api/analysis/ext/2/3/7", "/api/artwork/ext/2/3/7"]
            );
        }
    }
}
