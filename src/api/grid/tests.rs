// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde_json::Value;
use std::{io::Write, net::TcpListener};

struct Response {
    method: &'static str,
    path: &'static str,
    body: Value,
    status: &'static str,
    session: &'static str,
}
fn reply(method: &'static str, path: &'static str, body: Value) -> Response {
    Response {
        method,
        path,
        body,
        status: "200 OK",
        session: "one",
    }
}
fn fixture(responses: Vec<Response>) -> (Api, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    api.session = Some("one".into());
    let server = thread::spawn(move || {
        let mut payloads = vec![];
        for r in responses {
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
                    Err(e) => panic!("missing {} {}: {e}", r.method, r.path),
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
            let header = String::from_utf8(bytes).unwrap();
            assert!(
                header.starts_with(&format!("{} {} ", r.method, r.path)),
                "{header}"
            );
            assert!(
                header
                    .to_lowercase()
                    .contains("x-vynull-library-session: one")
            );
            let size = header
                .to_lowercase()
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length: ")
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(0);
            let mut bytes = vec![0; size];
            socket.read_exact(&mut bytes).unwrap();
            payloads.push(if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            });
            if r.status == "disconnect" {
                continue;
            }
            let body = r.body.to_string();
            write!(socket, "HTTP/1.1 {}\r\nX-Vynull-Library-Session: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", r.status, r.session, body.len()).unwrap();
        }
        payloads
    });
    (api, server)
}

fn before() -> Snapshot {
    Snapshot {
        raw: None,
        track: Track {
            id: 7,
            file_path: "/a.wav".into(),
            bpm: 120.0,
            duration: 3,
            ..Default::default()
        },
        settings: Settings {
            bpm: 120.0,
            beat_phase_shift: 0,
        },
        bpm: 120.0,
        duration_sec: 3,
        grid: BeatGrid::new(
            TrackAnalysis {
                track_id: 7,
                beats: [125.0, 625.0, 1135.0, 1660.0, 2185.0, 2715.0]
                    .into_iter()
                    .enumerate()
                    .map(|(i, time_ms)| crate::beat_grid::Beat {
                        time_ms,
                        beat_in_bar: (i % 4 + 1) as u8,
                    })
                    .collect(),
            },
            7,
        )
        .unwrap(),
    }
}
fn snapshot(s: &Snapshot) -> Vec<Response> {
    vec![
        reply(
            "GET",
            "/api/tracks/7",
            json!({"id":s.track.id,"file_path":s.track.file_path,"bpm":s.track.bpm,"duration":s.duration_sec,"analysis_locked":s.track.analysis_locked}),
        ),
        reply(
            "GET",
            "/api/tracks/7/beats",
            json!({"bpm":s.settings.bpm,"beat_phase_shift":s.settings.beat_phase_shift}),
        ),
        reply(
            "GET",
            "/api/analysis/7",
            json!({"grid_snapshot":s.raw,"track_id":7,"bpm":s.bpm,"duration_sec":s.duration_sec,"beats":s.grid.beats().iter().map(|b|json!({"time_ms":b.time_ms,"beat_in_bar":b.beat_in_bar})).collect::<Vec<_>>()}),
        ),
    ]
}
fn ack(method: &'static str, path: &'static str) -> Response {
    reply(method, path, json!({"ok":true}))
}

#[test]
fn grid_writes_lock_nudge_tempo_and_variable_downbeat_with_verified_readback() {
    for change in [
        GridChange::Lock(true),
        GridChange::Lock(false),
        GridChange::Nudge(-0.1),
        GridChange::Tempo(60.0),
        GridChange::Downbeat(1150.0),
        GridChange::Downbeat(1135.0),
    ] {
        let mut before = before();
        if let GridChange::Lock(locked) = change {
            before.track.analysis_locked = !locked;
        }
        before.raw = Some(crate::grid_edit::RawGrid::from_snapshot(&before));
        let after = change.apply(&before).unwrap();
        let mut replies = snapshot(&before);
        match change {
            GridChange::Lock(locked) => {
                replies.push(ack("PUT", "/api/tracks/7/lock"));
                replies.push(reply("GET", "/api/tracks/7/lock", json!({"locked":locked})));
            }
            GridChange::Nudge(_) => replies.push(ack("POST", "/api/analysis/beatgrid/adjust")),
            GridChange::Tempo(_) => {
                replies.push(ack("POST", "/api/analysis/beatgrid/adjust"));
                replies.push(ack("PUT", "/api/tracks/7/beats"));
            }
            GridChange::Downbeat(_) => {
                let (offset, _) = change.alignment(&before.grid).unwrap();
                if offset != 0.0 {
                    replies.push(ack("POST", "/api/analysis/beatgrid/adjust"));
                    replies.extend(snapshot(&GridChange::Nudge(offset).apply(&before).unwrap()));
                }
                replies.push(ack("PUT", "/api/tracks/7/beats"));
            }
        }
        replies.extend(snapshot(&after));
        let (api, server) = fixture(replies);
        assert!(
            api.edit_grid(&before.track, Some(&before), Some(change))
                .unwrap()
                .matches(&after)
        );
        let writes: Vec<_> = server
            .join()
            .unwrap()
            .into_iter()
            .filter(|v| !v.is_null())
            .collect();
        match change {
            GridChange::Lock(locked) => assert_eq!(writes, vec![json!({"locked":locked})]),
            GridChange::Nudge(ms) => assert_eq!(writes, vec![json!({"track_id":7,"offset_ms":ms})]),
            GridChange::Tempo(bpm) => assert_eq!(
                writes,
                vec![json!({"track_id":7,"bpm":bpm}), json!({"bpm":bpm})]
            ),
            GridChange::Downbeat(_) => {
                assert_eq!(writes.last().unwrap(), &json!({"beat_phase_shift":2}))
            }
        }
        assert!(writes.iter().all(|v| v.get("set_downbeat_ms").is_none()));
    }
}

#[test]
fn grid_rejects_locks_changed_grids_sources_sessions_and_missing_identity_without_writing() {
    for change in [GridChange::Nudge(1.0), GridChange::Lock(true)] {
        for case in [
            "locked",
            "changed",
            "phase",
            "source",
            "session",
            "unsupported",
        ] {
            let before = before();
            let mut actual = before.clone();
            match case {
                "locked" => actual.track.analysis_locked = true,
                "changed" => actual = GridChange::Nudge(1.0).apply(&actual).unwrap(),
                "phase" => actual.settings.beat_phase_shift = 1,
                "source" => actual.track.file_path = "/other.wav".into(),
                _ => (),
            }
            let mut replies = snapshot(&actual);
            if case == "source" {
                replies.truncate(1);
            }
            if case == "session" {
                replies.truncate(1);
                replies[0].session = "two";
            }
            if case == "unsupported" {
                replies.clear();
            }
            let (mut api, server) = fixture(replies);
            if case == "unsupported" {
                api.session = None;
            }
            assert!(
                api.edit_grid(&before.track, Some(&before), Some(change))
                    .is_err()
            );
            assert!(server.join().unwrap().iter().all(Value::is_null));
        }
    }
}

#[test]
fn grid_partial_writes_and_lost_replies_are_never_retried_and_require_reload() {
    for case in [
        "disconnect",
        "failed",
        "mid-conflict",
        "bad-readback",
        "bad-receipt",
        "bad-final",
        "locked-midway",
    ] {
        let before = before();
        let mut replies = snapshot(&before);
        let mut write = ack("POST", "/api/analysis/beatgrid/adjust");
        match case {
            "disconnect" => write.status = "disconnect",
            "failed" => write.status = "500 Internal Server Error",
            "bad-receipt" => write.body = json!({"ok":false}),
            _ => (),
        }
        replies.push(write);
        if matches!(
            case,
            "mid-conflict" | "bad-readback" | "bad-final" | "locked-midway"
        ) {
            let mut shifted = GridChange::Nudge(15.0).apply(&before).unwrap();
            if case == "bad-readback" {
                shifted = before.clone();
            }
            if case == "locked-midway" {
                shifted.track.analysis_locked = true;
            }
            replies.extend(snapshot(&shifted));
            if matches!(case, "mid-conflict" | "bad-final") {
                let mut rotate = ack("PUT", "/api/tracks/7/beats");
                if case == "mid-conflict" {
                    rotate.status = "409 Conflict";
                }
                replies.push(rotate);
                if case == "bad-final" {
                    replies.extend(snapshot(&shifted));
                }
            }
        }
        let (api, server) = fixture(replies);
        let error = api
            .edit_grid(
                &before.track,
                Some(&before),
                Some(GridChange::Downbeat(1150.0)),
            )
            .unwrap_err();
        assert!(error.contains("Reload grid"), "{error}");
        let writes = server
            .join()
            .unwrap()
            .into_iter()
            .filter(|v| !v.is_null())
            .count();
        assert_eq!(
            writes,
            if matches!(case, "mid-conflict" | "bad-final") {
                2
            } else {
                1
            }
        );
    }
}

#[test]
fn analysis_lock_requires_ack_endpoint_and_track_readback_without_retrying() {
    for case in [
        "disconnect",
        "rejected",
        "unsupported",
        "bad-receipt",
        "wrong-lock",
        "missing-lock",
        "lost-lock",
        "session",
        "wrong-track-lock",
        "changed-grid",
        "changed-source",
    ] {
        let before = before();
        let mut after = GridChange::Lock(true).apply(&before).unwrap();
        let mut replies = snapshot(&before);
        let mut write = ack("PUT", "/api/tracks/7/lock");
        match case {
            "disconnect" => write.status = "disconnect",
            "rejected" => write.status = "409 Conflict",
            "unsupported" => write.status = "404 Not Found",
            "bad-receipt" => write.body = json!({"ok":false}),
            _ => (),
        }
        replies.push(write);
        if !matches!(
            case,
            "disconnect" | "rejected" | "unsupported" | "bad-receipt"
        ) {
            let mut read = reply("GET", "/api/tracks/7/lock", json!({"locked":true}));
            match case {
                "wrong-lock" => read.body = json!({"locked":false}),
                "missing-lock" => read.body = json!({}),
                "lost-lock" => read.status = "disconnect",
                "session" => read.session = "two",
                "wrong-track-lock" => after.track.analysis_locked = false,
                "changed-grid" => after.grid = GridChange::Nudge(1.0).apply(&before).unwrap().grid,
                "changed-source" => after.track.file_path = "/other.wav".into(),
                _ => (),
            }
            replies.push(read);
            if matches!(case, "wrong-track-lock" | "changed-grid" | "changed-source") {
                let mut final_read = snapshot(&after);
                if case == "changed-source" {
                    final_read.truncate(1);
                }
                replies.extend(final_read);
            }
        }
        let (api, server) = fixture(replies);
        let error = api
            .edit_grid(&before.track, Some(&before), Some(GridChange::Lock(true)))
            .unwrap_err();
        assert!(error.contains("Reload grid"), "{case}: {error}");
        let writes: Vec<_> = server
            .join()
            .unwrap()
            .into_iter()
            .filter(|v| !v.is_null())
            .collect();
        assert_eq!(writes, vec![json!({"locked":true})], "{case}");
    }
}

#[test]
fn undo_redo_restores_exact_raw_grid_with_preflight_and_readback() {
    let mut original = before();
    original.raw = Some(crate::grid_edit::RawGrid::from_snapshot(&original));
    let edited = GridChange::Tempo(60.0).apply(&original).unwrap();
    for (before, after) in [(&edited, &original), (&original, &edited)] {
        let mut replies = snapshot(before);
        replies.push(ack("POST", "/api/analysis/beatgrid/restore"));
        replies.extend(snapshot(after));
        let (api, server) = fixture(replies);
        assert!(
            api.restore_grid(&before.track, before, after)
                .unwrap()
                .matches(after)
        );
        let writes: Vec<_> = server
            .join()
            .unwrap()
            .into_iter()
            .filter(|v| !v.is_null())
            .collect();
        assert_eq!(
            writes,
            vec![json!({"track_id":7,"expected":before.raw,"restore":after.raw})]
        );
    }
}

#[test]
fn history_restore_rejects_stale_state_and_never_retries_an_uncertain_write() {
    for case in [
        "changed",
        "locked",
        "unsupported",
        "disconnect",
        "conflict",
        "bad-readback",
        "bad-receipt",
        "session",
        "source",
    ] {
        let mut original = before();
        original.raw = Some(crate::grid_edit::RawGrid::from_snapshot(&original));
        let edited = GridChange::Tempo(60.0).apply(&original).unwrap();
        let mut actual = edited.clone();
        match case {
            "changed" => actual = GridChange::Nudge(1.0).apply(&edited).unwrap(),
            "locked" => actual.track.analysis_locked = true,
            "unsupported" => actual.raw = None,
            "source" => actual.track.file_path = "/other.wav".into(),
            _ => (),
        }
        let mut replies = snapshot(&actual);
        if case == "source" {
            replies.truncate(1);
        }
        if case == "session" {
            replies.truncate(1);
            replies[0].session = "two";
        }
        let writing = matches!(
            case,
            "disconnect" | "conflict" | "bad-readback" | "bad-receipt"
        );
        if writing {
            let mut write = ack("POST", "/api/analysis/beatgrid/restore");
            match case {
                "disconnect" => write.status = "disconnect",
                "conflict" => write.status = "409 Conflict",
                "bad-receipt" => write.body = json!({"ok":false}),
                _ => (),
            }
            replies.push(write);
            if case == "bad-readback" {
                replies.extend(snapshot(&edited));
            }
        }
        let (api, server) = fixture(replies);
        assert!(
            api.restore_grid(&edited.track, &edited, &original)
                .unwrap_err()
                .contains("Reload grid")
        );
        assert_eq!(
            server
                .join()
                .unwrap()
                .iter()
                .filter(|v| !v.is_null())
                .count(),
            usize::from(writing)
        );
    }
}
