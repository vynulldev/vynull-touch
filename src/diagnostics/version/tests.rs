// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

fn serve(status: u16, body: &str, length: Option<u64>) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let body = body.to_owned();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        let mut request = Vec::new();
        let mut byte = [0];
        while !request.ends_with(b"\r\n\r\n") {
            socket.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        assert!(
            String::from_utf8(request)
                .unwrap()
                .starts_with("GET /api/version HTTP/1.1")
        );
        write!(socket, "HTTP/1.1 {status} Test\r\nConnection: close\r\n").unwrap();
        if let Some(length) = length {
            write!(socket, "Content-Length: {length}\r\n").unwrap();
        }
        write!(socket, "\r\n").unwrap();
        let _ = socket.write_all(body.as_bytes());
    });
    (origin, server)
}

#[test]
fn version_contract_handles_older_servers_and_invalid_or_oversized_responses() {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let body = r#"{"version":"v0.6.1","commit":"abcdef0123456789","date":"2026-10-13T00:16:50Z","dirty":true,"go_version":"go1.26"}"#;
    let (origin, server) = serve(200, body, Some(body.len() as u64));
    let info = fetch(&client, &origin).unwrap().unwrap();
    assert_eq!(info.description(), "v0.6.1 · abcdef012345 · modified");
    assert_eq!(info.date, "2026-10-13T00:16:50Z");
    server.join().unwrap();
    for status in [404, 405, 501] {
        let (origin, server) = serve(status, "", Some(0));
        assert!(fetch(&client, &origin).unwrap().is_none());
        server.join().unwrap();
    }
    for (status, body, length, expected) in [
        (503, "unavailable".to_owned(), None, "HTTP 503"),
        (200, "null".into(), None, "Invalid server version"),
        (200, "{}".into(), None, "Invalid server version"),
        (
            200,
            r#"{"version":""}"#.into(),
            None,
            "Invalid server version",
        ),
        (200, String::new(), Some(LIMIT + 1), "exceeds 64 KiB"),
        (200, "x".repeat(LIMIT as usize + 1), None, "exceeds 64 KiB"),
    ] {
        let (origin, server) = serve(status, &body, length);
        assert!(fetch(&client, &origin).unwrap_err().contains(expected));
        server.join().unwrap();
    }
}

#[test]
fn version_polling_preserves_last_info_on_failure_and_does_no_work_when_hidden_or_demo() {
    let ctx = Context::default();
    let mut version = Version::default();
    version.update(&ctx, "invalid", false, false);
    version.update(&ctx, "invalid", true, true);
    assert!(version.pending.is_none());
    let (send, receive) = mpsc::sync_channel(1);
    version.pending = Some(receive);
    for _ in 0..5 {
        version.update(&ctx, "invalid", false, true);
    }
    send.send(Ok(Some(Info {
        version: "v1".into(),
        commit: String::new(),
        date: String::new(),
        dirty: false,
    })))
    .unwrap();
    version.update(&ctx, "invalid", false, true);
    assert!(version.pending.is_none());
    assert_eq!(version.description(false), "v1");
    let updated = version.updated;
    let (send, receive) = mpsc::sync_channel(1);
    version.pending = Some(receive);
    send.send(Err("offline".into())).unwrap();
    version.update(&ctx, "invalid", false, true);
    assert_eq!(version.description(false), "v1");
    assert_eq!(version.updated, updated);
    assert_eq!(version.failures, 1);
    assert_eq!(version.error, "offline");
    let (send, receive) = mpsc::sync_channel(1);
    version.pending = Some(receive);
    send.send(Ok(None)).unwrap();
    version.update(&ctx, "invalid", false, true);
    assert_eq!(version.description(false), "Not reported by this server");
    assert!(version.error.is_empty());
    assert!(version.info.is_none());
}
