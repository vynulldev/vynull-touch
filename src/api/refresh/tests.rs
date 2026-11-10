// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

fn fixture(
    replies: Vec<(&'static str, &'static str, &'static str, &'static str)>,
) -> (Api, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    api.session = Some("one".into());
    let server = thread::spawn(move || {
        for (path, status, session, body) in replies {
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
            assert!(request.starts_with(&format!("get {path} ")), "{request}");
            assert!(request.contains("x-vynull-library-session: one"));
            write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
    });
    (api, server)
}

fn ok(
    path: &'static str,
    body: &'static str,
) -> (&'static str, &'static str, &'static str, &'static str) {
    (path, "200 OK", "one", body)
}

#[test]
fn smart_playlist_counts_use_evaluated_membership_and_reject_other_sessions() {
    let (api, server) = fixture(vec![
        ok("/api/playlists/9/tracks", "null"),
        ok(
            "/api/playlists/10/tracks",
            r#"[{"id":2,"title":"Matches"},{"id":3}]"#,
        ),
        (
            "/api/playlists/11/tracks",
            "200 OK",
            "two",
            r#"[{"id":99}]"#,
        ),
        (
            "/api/playlists/12/tracks",
            "503 Service Unavailable",
            "one",
            "{}",
        ),
    ]);
    let ctx = egui::Context::default();
    let mut counts = crate::api::playlist_counts::Counts::default();
    let playlists: Vec<_> = (9..=12)
        .map(|id| Playlist {
            id,
            is_smart: true,
            ..Default::default()
        })
        .collect();
    for (id, expected) in [(9, Some(0)), (10, Some(2)), (11, None), (12, None)] {
        counts.request(id, api.origin.as_str(), api.session.as_deref(), &ctx);
        // Repeated UI frames must not enqueue duplicate or concurrent reads.
        counts.request(id, api.origin.as_str(), api.session.as_deref(), &ctx);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            counts.poll(&playlists);
            if counts.count(id).is_some() || counts.failed(id) {
                break;
            }
            assert!(Instant::now() < deadline, "count never completed");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(counts.count(id), expected);
        assert_eq!(counts.failed(id), expected.is_none());
        // Cached successes and failures both wait before retrying.
        counts.request(id, api.origin.as_str(), api.session.as_deref(), &ctx);
    }
    server.join().unwrap();
}

#[test]
fn unchanged_revisions_skip_collection_and_cues_but_check_open_smart_playlist() {
    let (api, server) = fixture(vec![
        ok("/api/tracks/rev", r#"{"rev":4,"library_session":"one"}"#),
        ok(
            "/api/playlists",
            r#"[{"id":9,"is_smart":true,"name":"New rules"}]"#,
        ),
        ok(
            "/api/playlists/9/tracks",
            r#"[{"id":2,"title":"Now matches"}]"#,
        ),
        ok("/api/cues/rev", r#"{"rev":7}"#),
    ]);
    let result = api
        .refresh(&Request {
            tracks_revision: Some(4),
            cues_revision: Some(7),
            source: Some(9),
            detail: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert!(result.tracks.is_none());
    assert!(result.cues.is_none());
    assert!(result.detail.is_none());
    assert_eq!(result.playlist_tracks.unwrap()[0].id, 2);
    server.join().unwrap();
}

#[test]
fn changed_revisions_fetch_new_rows_and_cue_deletions_and_keep_pre_read_revision() {
    let (api, server) = fixture(vec![
        ok("/api/tracks/rev", r#"{"rev":5,"library_session":"one"}"#),
        ok("/api/tracks", r#"[{"id":2,"title":"Renamed"}]"#),
        ok("/api/playlists", "[]"),
        ok("/api/cues/rev", r#"{"rev":8}"#),
        ok("/api/cues", r#"{"2":[]}"#),
    ]);
    let result = api
        .refresh(&Request {
            tracks_revision: Some(4),
            cues_revision: Some(7),
            source: Some(9),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(result.tracks.unwrap()[0].title, "Renamed");
    assert_eq!(result.tracks_revision, Some(5));
    assert_eq!(result.cues_revision, Some(8));
    assert!(result.cues.unwrap()[&2].is_empty());
    assert!(
        result.playlist_tracks.is_none(),
        "deleted playlist is not fetched"
    );
    server.join().unwrap();
}

#[test]
fn cue_only_change_does_not_download_waveforms_or_the_track_list() {
    let (api, server) = fixture(vec![
        ok("/api/tracks/rev", r#"{"rev":4,"library_session":"one"}"#),
        ok("/api/playlists", "[]"),
        ok("/api/cues/rev", r#"{"rev":8}"#),
        ok(
            "/api/cues",
            r#"{"1":[{"number":1,"type":"cue","time_ms":5000,"loop_ms":-1}]}"#,
        ),
    ]);
    let result = api
        .refresh(&Request {
            tracks_revision: Some(4),
            cues_revision: Some(7),
            detail: Some(1),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(result.cues.unwrap()[&1][0].time_ms, 5000);
    assert!(result.tracks.is_none());
    assert!(result.detail.is_none());
    server.join().unwrap();
}

#[test]
fn unsupported_cue_revisions_allow_library_refresh_and_manual_detail_reload() {
    let (api, server) = fixture(vec![
        ok("/api/tracks/rev", r#"{"rev":4,"library_session":"one"}"#),
        ok("/api/tracks", r#"[{"id":1}]"#),
        ok("/api/playlists", "[]"),
        ("/api/cues/rev", "404 Not Found", "one", "{}"),
        ok(
            "/api/tracks/1/cues",
            r#"[{"number":1,"type":"cue","time_ms":9000,"loop_ms":-1}]"#,
        ),
        ok(
            "/api/analysis/waveform/1?type=detail&style=spectral",
            r#"{"track_id":1,"sample_rate":150,"entries":[{"r":1,"g":2,"b":3,"h":20}]}"#,
        ),
        ok("/api/analysis/1", r#"{"track_id":1,"beats":[]}"#),
        (
            "/api/analysis/waveform/1?type=detail&style=3band",
            "404 Not Found",
            "one",
            "{}",
        ),
        ok("/api/tracks/1/phrases", "[]"),
    ]);
    let result = api
        .refresh(&Request {
            tracks_revision: Some(4),
            detail: Some(1),
            force: true,
            ..Default::default()
        })
        .unwrap();
    assert!(result.skip_cues);
    assert_eq!(result.detail.unwrap().cues[0].time_ms, 9000);
    server.join().unwrap();
}

#[test]
fn failed_snapshot_does_not_advance_validators_and_session_changes_fail_closed() {
    for session_change in [false, true] {
        let replies = if session_change {
            vec![(
                "/api/tracks/rev",
                "200 OK",
                "two",
                r#"{"rev":4,"library_session":"two"}"#,
            )]
        } else {
            vec![
                ok("/api/tracks/rev", r#"{"rev":5,"library_session":"one"}"#),
                ("/api/tracks", "503 Service Unavailable", "one", "{}"),
            ]
        };
        let (api, server) = fixture(replies);
        let request = Request {
            tracks_revision: Some(4),
            ..Default::default()
        };
        let error = api.refresh(&request).err().unwrap();
        assert_eq!(request.tracks_revision, Some(4));
        assert_eq!(error.starts_with("Library changed"), session_change);
        server.join().unwrap();
    }
}
