// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
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

fn track(rating: u8, color: u8) -> Value {
    json!({"id":7,"title":"Test","file_path":"/test.flac","rating":rating,"color_id":color})
}
fn tags() -> Value {
    json!([{"id":1,"name":"Warmup"},{"id":2,"name":"Other"}])
}
fn before() -> TrackSnapshot {
    TrackSnapshot {
        track: serde_json::from_value(track(1, 0)).unwrap(),
        catalog: Catalog {
            tags: serde_json::from_value(tags()).unwrap(),
            categories: vec![],
        },
        tags: BTreeSet::from([1]),
    }
}
fn snapshot(rating: u8, color: u8, assigned: Value) -> Vec<Response> {
    vec![
        reply("GET", "/api/tracks/7", track(rating, color)),
        reply("GET", "/api/tags", tags()),
        reply("GET", "/api/tag-categories", json!([])),
        reply("GET", "/api/tracks/7/tags", assigned),
    ]
}

#[test]
fn quick_edits_write_only_changes_preserve_independent_tags_and_verify_readback() {
    let mut replies = snapshot(1, 0, tags());
    replies.extend([
        reply("PUT", "/api/tracks/7/rating", json!({"ok":true})),
        reply("PUT", "/api/tracks/7/color", json!({"ok":true})),
        reply("POST", "/api/tracks/tags", json!({"updated":1})),
    ]);
    replies.extend(snapshot(5, 4, json!([{"id":2,"name":"Other"}])));
    let (api, server) = fixture(replies);
    let result = api
        .edit(&Request::SaveTrack {
            before: Box::new(before()),
            rating: 5,
            color: 4,
            tags: BTreeSet::new(),
        })
        .unwrap();
    let Reply::Track(after) = result else {
        panic!()
    };
    assert_eq!(after.tags, BTreeSet::from([2]));
    assert_eq!(after.track.rating, 5);
    let payloads = server.join().unwrap();
    assert_eq!(payloads[4], json!({"rating":5}));
    assert_eq!(payloads[5], json!({"color_id":4}));
    assert_eq!(payloads[6], json!({"track_ids":[7],"add":[],"remove":[1]}));
}

#[test]
fn changed_rating_and_lost_write_response_never_replay_or_send_later_writes() {
    let request = Request::SaveTrack {
        before: Box::new(before()),
        rating: 5,
        color: 4,
        tags: BTreeSet::from([1]),
    };
    let (api, server) = fixture(snapshot(3, 0, json!([{"id":1,"name":"Warmup"}])));
    assert!(
        api.edit(&request)
            .err()
            .unwrap()
            .contains("changed elsewhere")
    );
    assert_eq!(server.join().unwrap().len(), 4);
    let mut replies = snapshot(1, 0, json!([{"id":1,"name":"Warmup"}]));
    let mut lost = reply("PUT", "/api/tracks/7/rating", Value::Null);
    lost.status = "disconnect";
    replies.push(lost);
    let (api, server) = fixture(replies);
    assert!(
        api.edit(&request)
            .err()
            .unwrap()
            .contains("some changes may have applied")
    );
    assert_eq!(server.join().unwrap().len(), 5);
}

#[test]
fn tag_creation_and_category_deletion_use_ids_and_preserve_assignments() {
    let tag = json!({"id":3,"name":"Peak","category_id":9});
    let category = json!({"id":9,"name":"Energy"});
    let (api, server) = fixture(vec![
        reply("GET", "/api/tags", json!([])),
        reply("GET", "/api/tag-categories", json!([category.clone()])),
        reply("POST", "/api/tags", tag.clone()),
        reply("GET", "/api/tags", json!([tag.clone()])),
        reply("GET", "/api/tag-categories", json!([category.clone()])),
        reply("GET", "/api/tracks", json!([])),
        reply("GET", "/api/tags", json!([tag.clone()])),
        reply("GET", "/api/tag-categories", json!([category.clone()])),
        reply("DELETE", "/api/tag-categories/9", json!({"ok":true})),
        reply(
            "GET",
            "/api/tags",
            json!([{"id":3,"name":"Peak","category_id":0}]),
        ),
        reply("GET", "/api/tag-categories", json!([])),
        reply("GET", "/api/tracks", json!([])),
    ]);
    let category: Category = serde_json::from_value(category).unwrap();
    api.edit(&Request::Tag {
        before: None,
        name: "Peak".into(),
        category: Some(category.clone()),
    })
    .unwrap();
    let result = api
        .edit(&Request::DeleteCategory {
            before: category,
            tags: vec![serde_json::from_value(tag).unwrap()],
        })
        .unwrap();
    let Reply::Catalog(after, _) = result else {
        panic!()
    };
    assert_eq!(after.tags[0].category_id, 0);
    let payloads = server.join().unwrap();
    assert_eq!(payloads[2], json!({"name":"Peak","category_id":9}));
}

#[test]
fn playlist_reordering_is_verified_and_rejects_changed_or_read_only_membership() {
    let old = json!([{"id":9,"name":"Set","track_ids":[7,8,10]}]);
    let mut before: Playlist = serde_json::from_value(old[0].clone()).unwrap();
    let (api, server) = fixture(vec![
        reply("GET", "/api/playlists", old.clone()),
        reply("POST", "/api/playlists/9/tracks", json!({"ok":true})),
        reply(
            "GET",
            "/api/playlists",
            json!([{"id":9,"name":"Set","track_ids":[10,7]}]),
        ),
    ]);
    api.edit(&Request::Members {
        before: before.clone(),
        ids: vec![10, 7],
        added: vec![],
    })
    .unwrap();
    assert_eq!(server.join().unwrap()[1], json!({"track_ids":[10,7]}));
    for smart in [false, true] {
        before.is_smart = smart;
        let (api, server) = fixture(vec![reply(
            "GET",
            "/api/playlists",
            json!([{"id":9,"name":"Set","is_smart":smart,"track_ids":[7,8,10,11]}]),
        )]);
        assert!(
            api.edit(&Request::Members {
                before: before.clone(),
                ids: vec![10, 7],
                added: vec![]
            })
            .is_err()
        );
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[test]
fn source_and_session_changes_block_edits_and_missing_readback_is_not_success() {
    let mut r = reply("GET", "/api/tracks/7", track(1, 0));
    r.session = "two";
    let (api, server) = fixture(vec![r]);
    assert!(
        api.edit(&Request::SaveTrack {
            before: Box::new(before()),
            rating: 2,
            color: 0,
            tags: BTreeSet::from([1])
        })
        .err()
        .unwrap()
        .contains("Library changed")
    );
    server.join().unwrap();
    let mut other = track(1, 0);
    other["file_path"] = json!("/replacement.flac");
    let (api, server) = fixture(vec![reply("GET", "/api/tracks/7", other)]);
    assert!(api.edit(&Request::Track(before().track)).is_err());
    server.join().unwrap();
    let mut replies = snapshot(1, 0, json!([{"id":1,"name":"Warmup"}]));
    replies.push(reply("PUT", "/api/tracks/7/rating", json!({"ok":true})));
    replies.extend(snapshot(1, 0, json!([{"id":1,"name":"Warmup"}])));
    let (api, server) = fixture(replies);
    assert!(
        api.edit(&Request::SaveTrack {
            before: Box::new(before()),
            rating: 2,
            color: 0,
            tags: BTreeSet::from([1])
        })
        .err()
        .unwrap()
        .contains("Readback differs")
    );
    server.join().unwrap();
}

#[test]
fn playlist_additions_preserve_concurrent_entries_and_check_track_identity() {
    let before: Playlist =
        serde_json::from_value(json!({"id":9,"name":"Set","track_ids":[8]})).unwrap();
    let request = Request::Members {
        before,
        ids: vec![8, 7],
        added: vec![serde_json::from_value(track(1, 0)).unwrap()],
    };
    let (api, server) = fixture(vec![
        reply(
            "GET",
            "/api/playlists",
            json!([{"id":9,"name":"Set","track_ids":[8]}]),
        ),
        reply("GET", "/api/tracks/7", track(1, 0)),
        reply("POST", "/api/tracks/playlists", json!({"updated":1})),
        reply(
            "GET",
            "/api/playlists",
            json!([{"id":9,"name":"Set","track_ids":[8,99,7]}]),
        ),
    ]);
    let Reply::Playlists(after) = api.edit(&request).unwrap() else {
        panic!()
    };
    assert_eq!(after[0].track_ids, [8, 99, 7]);
    assert_eq!(
        server.join().unwrap()[2],
        json!({"track_ids":[7],"add":[9],"remove":[]})
    );
}

#[test]
fn playlist_create_rename_and_recursive_delete_verify_canonical_tree() {
    let folder = json!({"id":30,"name":"Sets","is_folder":true});
    let child = json!({"id":31,"name":"Opening","parent_id":30});
    let renamed = json!({"id":31,"name":"Closing","parent_id":0});
    let (api, server) = fixture(vec![
        reply("GET", "/api/playlists", json!([])),
        reply("POST", "/api/playlists", folder.clone()),
        reply("GET", "/api/playlists", json!([folder.clone()])),
        reply(
            "GET",
            "/api/playlists",
            json!([folder.clone(), child.clone()]),
        ),
        reply("PUT", "/api/playlists/31", json!({"ok":true})),
        reply(
            "GET",
            "/api/playlists",
            json!([folder.clone(), renamed.clone()]),
        ),
        reply(
            "GET",
            "/api/playlists",
            json!([folder.clone(), child.clone()]),
        ),
        reply("DELETE", "/api/playlists/30", json!({"ok":true})),
        reply("GET", "/api/playlists", json!([])),
    ]);
    api.edit(&Request::Playlist {
        before: None,
        name: "Sets".into(),
        parent: None,
        folder: true,
    })
    .unwrap();
    api.edit(&Request::Playlist {
        before: Some(serde_json::from_value(child.clone()).unwrap()),
        name: "Closing".into(),
        parent: None,
        folder: false,
    })
    .unwrap();
    let tree = serde_json::from_value(json!([folder, child])).unwrap();
    let Reply::Playlists(after) = api
        .edit(&Request::DeletePlaylist {
            id: 30,
            before: tree,
        })
        .unwrap()
    else {
        panic!()
    };
    assert!(after.is_empty());
    let payloads = server.join().unwrap();
    assert_eq!(
        payloads[1],
        json!({"name":"Sets","parent_id":0,"is_folder":true})
    );
    assert_eq!(payloads[4], json!({"name":"Closing","parent_id":0}));
}

#[test]
fn tag_rename_delete_and_read_only_playlist_guards_are_verified() {
    let old: Tag = serde_json::from_value(json!({"id":1,"name":"Warmup"})).unwrap();
    let renamed: Tag = serde_json::from_value(json!({"id":1,"name":"Opening"})).unwrap();
    let (api, server) = fixture(vec![
        reply("GET", "/api/tags", json!([{"id":1,"name":"Warmup"}])),
        reply("GET", "/api/tag-categories", json!([])),
        reply("PUT", "/api/tags/1", json!({"ok":true})),
        reply("GET", "/api/tags", json!([{"id":1,"name":"Opening"}])),
        reply("GET", "/api/tag-categories", json!([])),
        reply("GET", "/api/tracks", json!([])),
        reply("GET", "/api/tags", json!([{"id":1,"name":"Opening"}])),
        reply("GET", "/api/tag-categories", json!([])),
        reply("DELETE", "/api/tags/1", json!({"ok":true})),
        reply("GET", "/api/tags", json!([])),
        reply("GET", "/api/tag-categories", json!([])),
        reply("GET", "/api/tracks", json!([])),
    ]);
    api.edit(&Request::Tag {
        before: Some(old),
        name: "Opening".into(),
        category: None,
    })
    .unwrap();
    api.edit(&Request::DeleteTag(renamed)).unwrap();
    assert_eq!(
        server.join().unwrap()[2],
        json!({"name":"Opening","category_id":0})
    );
    let usb: Playlist =
        serde_json::from_value(json!({"id":1073741825,"name":"USB","track_ids":[7]})).unwrap();
    let (api, server) = fixture(vec![reply(
        "GET",
        "/api/playlists",
        json!([{"id":1073741825,"name":"USB","track_ids":[7]}]),
    )]);
    assert!(
        api.edit(&Request::Members {
            before: usb,
            ids: vec![],
            added: vec![]
        })
        .is_err()
    );
    server.join().unwrap();
}
