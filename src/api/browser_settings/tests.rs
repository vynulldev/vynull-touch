// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde_json::json;
use std::{io::Write, net::TcpListener};

type Reply = (&'static str, Value, &'static str, &'static str);
fn get(snapshot: &Snapshot) -> Reply {
    ("GET", json!(snapshot), "200 OK", "one")
}
fn ack() -> Reply {
    ("PUT", json!({"ok":true}), "200 OK", "one")
}

fn fixture(replies: Vec<Reply>) -> (Api, thread::JoinHandle<Vec<Value>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    api.session = Some("one".into());
    let handle = thread::spawn(move || {
        let mut bodies = vec![];
        for (method, body, status, session) in replies {
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
                    Err(e) => panic!("missing {method}: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(4)))
                .unwrap();
            let mut header = vec![];
            let mut byte = [0];
            while !header.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            let header = String::from_utf8(header).unwrap().to_lowercase();
            assert!(header.starts_with(&format!("{} /api/menu-items ", method.to_lowercase())));
            assert!(header.contains("x-vynull-library-session: one"));
            let size: usize = header
                .lines()
                .find_map(|l| {
                    l.strip_prefix("content-length: ")
                        .map(|s| s.parse().unwrap())
                })
                .unwrap_or(0);
            let mut bytes = vec![0; size];
            socket.read_exact(&mut bytes).unwrap();
            bodies.push(if bytes.is_empty() {
                Value::Null
            } else {
                serde_json::from_slice(&bytes).unwrap()
            });
            if status == "disconnect" {
                continue;
            }
            let body = body.to_string();
            write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        }
        bodies
    });
    (api, handle)
}

fn edit(before: &Snapshot) -> Edit {
    let mut draft = before.clone();
    draft.move_category("album", -1);
    draft.toggle_category("artist");
    draft.move_sort(1, -1);
    draft.toggle_sort(2);
    draft.track_detail = "artist".into();
    Edit {
        before: before.clone(),
        draft,
    }
}

#[test]
fn browser_save_updates_all_components_with_exact_payload_and_readback() {
    let before = Snapshot::demo();
    let edit = edit(&before);
    let (body, target) = edit.prepare(&before).unwrap();
    let (api, server) = fixture(vec![get(&before), ack(), get(&target)]);
    assert_eq!(api.save_browser_settings(&edit).unwrap(), target);
    let payloads = server.join().unwrap();
    assert_eq!(payloads, [Value::Null, body, Value::Null]);
    assert!(payloads[1]["items"][0].get("locked").is_none());
    assert!(payloads[1]["sorts"][0].get("label").is_none());
}

#[test]
fn browser_save_preserves_changes_to_untouched_components() {
    let before = Snapshot::demo();
    let mut draft = before.clone();
    draft.track_detail = "album".into();
    let edit = Edit {
        before: before.clone(),
        draft,
    };
    let mut latest = before;
    latest.move_category("album", -1);
    latest.toggle_sort(0);
    let (_, target) = edit.prepare(&latest).unwrap();
    let (api, server) = fixture(vec![get(&latest), ack(), get(&target)]);
    assert_eq!(api.save_browser_settings(&edit).unwrap(), target);
    assert_eq!(server.join().unwrap()[1], json!({"track_detail":"album"}));
}

#[test]
fn browser_conflicts_or_changed_sessions_prevent_put() {
    let before = Snapshot::demo();
    let mut latest = before.clone();
    latest.toggle_category("key");
    let (api, server) = fixture(vec![get(&latest)]);
    assert!(
        api.save_browser_settings(&edit(&before))
            .unwrap_err()
            .contains("changed on Vynull")
    );
    assert_eq!(server.join().unwrap(), [Value::Null]);
    let (api, server) = fixture(vec![("GET", json!(before), "200 OK", "two")]);
    assert!(
        api.save_browser_settings(&edit(&before))
            .unwrap_err()
            .contains("Library changed")
    );
    assert_eq!(server.join().unwrap(), [Value::Null]);
}

#[test]
fn browser_partial_or_lost_write_is_never_replayed() {
    let before = Snapshot::demo();
    for status in ["disconnect", "400 Bad Request", "500 Internal Server Error"] {
        let (api, server) = fixture(vec![
            get(&before),
            (
                "PUT",
                json!({"error":"sorts failed after categories saved"}),
                status,
                "one",
            ),
        ]);
        assert!(
            api.save_browser_settings(&edit(&before))
                .unwrap_err()
                .contains("some changes may have applied")
        );
        assert_eq!(server.join().unwrap().len(), 2);
    }
}

#[test]
fn browser_write_requires_matching_successful_readback() {
    let before = Snapshot::demo();
    for response in [
        get(&before),
        ("GET", Value::Null, "503 Service Unavailable", "one"),
    ] {
        let (api, server) = fixture(vec![get(&before), ack(), response]);
        assert!(api.save_browser_settings(&edit(&before)).is_err());
        server.join().unwrap();
    }
}

#[test]
fn browser_missing_or_incomplete_endpoint_never_creates_defaults() {
    for (body, status) in [
        (json!({"error":"missing"}), "404 Not Found"),
        (json!({"items":[]}), "200 OK"),
    ] {
        let (api, server) = fixture(vec![("GET", body, status, "one")]);
        assert!(api.browser_settings().is_err());
        assert_eq!(server.join().unwrap(), [Value::Null]);
    }
}
