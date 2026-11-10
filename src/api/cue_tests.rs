// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

fn fixture(
    replies: Vec<(&'static str, &'static str, String)>,
) -> (Api, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    api.session = Some("one".into());
    let task = thread::spawn(move || {
        let mut requests = vec![];
        for (status, session, body) in replies {
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
                    Err(e) => panic!("missing request: {e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = vec![];
            let mut byte = [0];
            while !data.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                data.push(byte[0]);
            }
            let headers = String::from_utf8(data.clone()).unwrap().to_lowercase();
            let len = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let mut payload = vec![0; len];
            socket.read_exact(&mut payload).unwrap();
            data.extend(payload);
            requests.push(String::from_utf8(data).unwrap());
            if status != "disconnect" {
                write!(socket,"HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            }
        }
        requests
    });
    (api, task)
}
fn track() -> Track {
    Track {
        id: 7,
        file_path: "/track.wav".into(),
        duration: 120,
        ..Default::default()
    }
}
fn track_json() -> String {
    r#"{"id":7,"file_path":"/track.wav","duration":120}"#.into()
}
fn cue() -> Cue {
    Cue {
        number: 2,
        kind: "loop".into(),
        time_ms: 1125,
        loop_ms: 3125,
        color_id: 42,
        label: "Drop α".into(),
    }
}
fn list(cue: &Cue) -> String {
    serde_json::to_string(&vec![cue]).unwrap()
}
fn ok(body: String) -> (&'static str, &'static str, String) {
    ("200 OK", "one", body)
}

#[test]
fn cue_put_delete_and_explicit_retries_check_source_slot_session_and_readback() {
    for persisted in [false, true] {
        let desired = cue();
        let (api, server) = fixture(vec![
            ok(track_json()),
            ok("[]".into()),
            ok(format!("{{\"ok\":true,\"persisted\":{persisted}}}")),
            ok(list(&desired)),
        ]);
        let saved = api
            .edit_cue(&track(), None, Some(&Change::Put(desired.clone())))
            .unwrap();
        assert_eq!(saved.persisted, persisted);
        assert_eq!(saved.cues, vec![desired.clone()]);
        let requests = server.join().unwrap();
        assert!(
            requests
                .iter()
                .all(|r| r.to_lowercase().contains("x-vynull-library-session: one"))
        );
        assert!(requests[2].starts_with("POST /api/tracks/7/cues "));
        assert_eq!(
            serde_json::from_str::<Cue>(requests[2].split("\r\n\r\n").nth(1).unwrap()).unwrap(),
            desired
        );
    }
    for before in [list(&cue()), "[]".into()] {
        let (api, server) = fixture(vec![
            ok(track_json()),
            ok(before),
            ok(r#"{"ok":true,"persisted":true}"#.into()),
            ok("[]".into()),
        ]);
        let saved = api
            .edit_cue(&track(), Some(&cue()), Some(&Change::Delete(2)))
            .unwrap();
        assert!(saved.cues.is_empty() && saved.persisted);
        assert!(server.join().unwrap()[2].starts_with("DELETE /api/tracks/7/cues/2 "));
    }
}
#[test]
fn cue_conflicts_wrong_source_and_library_change_do_not_write() {
    let mut changed = cue();
    changed.label = "Other client".into();
    let (api, server) = fixture(vec![ok(track_json()), ok(list(&changed))]);
    assert!(
        api.edit_cue(&track(), Some(&cue()), Some(&Change::Delete(2)))
            .err()
            .unwrap()
            .contains("changed in another client")
    );
    assert_eq!(server.join().unwrap().len(), 2);
    let (api, server) = fixture(vec![ok(track_json())]);
    let mut source = track();
    source.file_path = "/different.wav".into();
    assert!(
        api.edit_cue(&source, None, Some(&Change::Put(cue())))
            .err()
            .unwrap()
            .starts_with("Library changed")
    );
    assert_eq!(server.join().unwrap().len(), 1);
    let (api, server) = fixture(vec![("200 OK", "two", track_json())]);
    assert!(
        api.edit_cue(&track(), None, Some(&Change::Put(cue())))
            .is_err()
    );
    assert_eq!(server.join().unwrap().len(), 1);
}
#[test]
fn cue_save_failures_are_not_retried_or_claimed_as_saved() {
    for status in ["500 Internal Server Error", "disconnect"] {
        let (api, server) = fixture(vec![
            ok(track_json()),
            ok("[]".into()),
            (status, "one", "disk failed".into()),
        ]);
        assert!(
            api.edit_cue(&track(), None, Some(&Change::Put(cue())))
                .err()
                .unwrap()
                .contains("Save not confirmed")
        );
        assert_eq!(server.join().unwrap().len(), 3);
    }
    let (api, server) = fixture(vec![
        ok(track_json()),
        ok("[]".into()),
        ok(r#"{"ok":true,"persisted":true}"#.into()),
        ok("[]".into()),
    ]);
    assert!(
        api.edit_cue(&track(), None, Some(&Change::Put(cue())))
            .err()
            .unwrap()
            .contains("readback differs")
    );
    assert_eq!(server.join().unwrap().len(), 4);
}
