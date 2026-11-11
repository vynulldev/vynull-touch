// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

type Reply = (&'static str, &'static str, Value, &'static str);

fn reads(snapshot: &Snapshot) -> Vec<Reply> {
    vec![
        (
            "GET",
            "/api/settings",
            json!({"full": snapshot.full}),
            "200 OK",
        ),
        (
            "GET",
            "/api/settings/enums",
            json!(snapshot.enums),
            "200 OK",
        ),
    ]
}

fn server(replies: Vec<Reply>) -> (Api, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    api.session = Some("session-one".into());
    let handle = thread::spawn(move || {
        let mut bodies = Vec::new();
        for (method, path, response, status) in replies {
            let deadline = Instant::now() + Duration::from_secs(4);
            let mut socket = loop {
                match listener.accept() {
                    Ok((socket, _)) => break socket,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(e) => panic!("Missing {method} {path}: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let mut header = Vec::new();
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let header = String::from_utf8(header).unwrap().to_lowercase();
            assert!(header.starts_with(&format!("{} {path} ", method.to_lowercase())));
            assert!(header.contains("x-vynull-library-session: session-one"));
            let size: usize = header
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length: ")
                        .map(|s| s.parse().unwrap())
                })
                .unwrap_or(0);
            let mut body = vec![0; size];
            socket.read_exact(&mut body).unwrap();
            bodies.push(if body.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&body).unwrap()
            });
            if status == "disconnect" {
                continue;
            }
            let body = response.to_string();
            write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: session-one\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        bodies
    });
    (api, handle)
}

fn edit(snapshot: &Snapshot) -> Vec<Change> {
    let mut draft = snapshot.full.clone();
    draft["dev_setting"]["waveform_color"] = json!("blue");
    draft["djm_my_setting"]["cross_fader_curve"] = json!("slow_cut");
    snapshot.changes(&draft).unwrap()
}

#[test]
fn device_settings_save_merges_fresh_values_and_verifies_readback() {
    let initial = Snapshot::demo();
    let changes = edit(&initial);
    let mut latest = initial.clone();
    latest.full["my_setting"]["auto_cue"] = json!("off");
    latest.full["grid_offset_ms"] = json!(17.5);
    latest.full["future"] = json!({"keep": [1,2,3]});
    let mut saved = latest.clone();
    saved.full = latest.merge(&changes).unwrap();
    let mut replies = reads(&latest);
    replies.push(("POST", "/api/settings", json!({"ok":true}), "200 OK"));
    replies.extend(reads(&saved));
    let (api, handle) = server(replies);
    assert_eq!(api.save_device_settings(&changes).unwrap().full, saved.full);
    let bodies = handle.join().unwrap();
    assert_eq!(bodies[2], json!({"full":saved.full}));
    assert_eq!(bodies.iter().filter(|v| !v.is_null()).count(), 1);
}

#[test]
fn device_settings_stale_edit_never_posts() {
    let mut fresh = Snapshot::demo();
    let changes = edit(&fresh);
    fresh.full["dev_setting"]["waveform_color"] = json!("3band");
    let (api, handle) = server(reads(&fresh));
    assert!(
        api.save_device_settings(&changes)
            .unwrap_err()
            .contains("changed on the server")
    );
    assert!(handle.join().unwrap().iter().all(Value::is_null));
}

#[test]
fn device_settings_ambiguous_post_is_not_retried_or_reported_saved() {
    for status in ["disconnect", "500 Internal Server Error"] {
        let snapshot = Snapshot::demo();
        let mut replies = reads(&snapshot);
        replies.push((
            "POST",
            "/api/settings",
            json!({"error":"write failed"}),
            status,
        ));
        let (api, handle) = server(replies);
        assert!(
            api.save_device_settings(&edit(&snapshot))
                .unwrap_err()
                .contains("not confirmed")
        );
        assert_eq!(handle.join().unwrap().len(), 3);
    }
}

#[test]
fn device_settings_success_requires_matching_readback() {
    let snapshot = Snapshot::demo();
    let mut replies = reads(&snapshot);
    replies.push(("POST", "/api/settings", json!({"ok":true}), "200 OK"));
    replies.extend(reads(&snapshot));
    let (api, handle) = server(replies);
    assert!(
        api.save_device_settings(&edit(&snapshot))
            .unwrap_err()
            .contains("did not match")
    );
    handle.join().unwrap();
}

#[test]
fn device_settings_legacy_response_cannot_seed_a_write() {
    let snapshot = Snapshot::demo();
    let mut replies = reads(&snapshot);
    replies[0].2 = json!({"waveform_color":"rgb"});
    let (api, handle) = server(replies);
    assert!(
        api.device_settings()
            .unwrap_err()
            .contains("complete CDJ/mixer settings")
    );
    handle.join().unwrap();
}
