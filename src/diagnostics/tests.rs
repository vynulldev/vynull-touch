// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

fn entry(seq: u64) -> Entry {
    Entry {
        seq,
        time: 1_794_052_800_000,
        line: format!("status: log {seq}"),
    }
}

fn response(
    status: u16,
    body: String,
    length: Option<u64>,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
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
        let request = String::from_utf8(request).unwrap();
        assert!(request.starts_with("GET /api/diag/logs?since=0 HTTP/1.1\r\n"));
        assert!(request.to_lowercase().contains("cache-control: no-cache"));
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
fn fetches_server_window_handles_null_and_reports_missing_or_invalid_endpoints() {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    let body = serde_json::to_string(
        &(1..=510)
            .map(|seq| serde_json::json!({"seq":seq,"time":0,"line":"log"}))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let (origin, server) = response(200, body.clone(), Some(body.len() as u64));
    let rows = fetch(&client, &origin).unwrap();
    assert_eq!(rows.len(), 500);
    assert_eq!((rows[0].seq, rows[499].seq), (11, 510));
    server.join().unwrap();
    for (status, body, expected) in [
        (200, "null", None),
        (200, "[]", None),
        (404, "not found", Some("does not provide")),
        (503, "unavailable", Some("HTTP 503")),
        (
            200,
            "<html>not JSON</html>",
            Some("Invalid server log response"),
        ),
        (
            200,
            r#"[{"seq":2,"time":0,"line":"first"},{"seq":1,"time":0,"line":"second"}]"#,
            Some("Invalid server log sequence"),
        ),
        (
            200,
            r#"[{"seq":0,"time":0,"line":"zero"}]"#,
            Some("Invalid server log sequence"),
        ),
    ] {
        let (origin, server) = response(status, body.into(), Some(body.len() as u64));
        let result = fetch(&client, &origin);
        if let Some(expected) = expected {
            assert!(result.unwrap_err().contains(expected));
        } else {
            assert!(result.unwrap().is_empty());
        }
        server.join().unwrap();
    }
}

#[test]
fn log_response_limits_apply_with_and_without_content_length() {
    let client = Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .unwrap();
    for (body, length) in [
        (String::new(), Some(MAX_BYTES + 1)),
        ("x".repeat(MAX_BYTES as usize + 1), None),
    ] {
        let (origin, server) = response(200, body, length);
        assert!(
            fetch(&client, &origin)
                .unwrap_err()
                .contains("exceeds 2 MiB")
        );
        server.join().unwrap();
    }
}

#[test]
fn polling_is_bounded_pauses_pending_replies_and_recovers_from_errors_and_restart() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (outgoing, replies) = mpsc::sync_channel(1);
    let mut logs = Logs {
        worker: Some(Worker { requests, replies }),
        ..Default::default()
    };
    let ctx = Context::default();
    logs.update(&ctx, "http://unused", false, false);
    assert!(incoming.try_recv().is_err());
    for _ in 0..10 {
        logs.update(&ctx, "http://unused", false, true);
    }
    let old_epoch = incoming.try_recv().unwrap();
    assert!(
        incoming.try_recv().is_err(),
        "only one request may be pending"
    );
    logs.pause();
    outgoing.send((old_epoch, Ok(vec![entry(50)]))).unwrap();
    logs.update(&ctx, "http://unused", false, true);
    assert!(
        logs.entries.is_empty(),
        "pause must freeze in-flight responses too"
    );
    logs.resume();
    logs.update(&ctx, "http://unused", false, true);
    let epoch = incoming.try_recv().unwrap();
    assert_ne!(epoch, old_epoch);
    outgoing.send((epoch, Ok(vec![entry(50)]))).unwrap();
    logs.update(&ctx, "http://unused", false, true);
    assert_eq!(logs.entries, vec![entry(50)]);
    assert_eq!(logs.revision, 1);
    assert!(logs.updated.is_some());
    logs.resume();
    logs.update(&ctx, "http://unused", false, true);
    incoming.try_recv().unwrap();
    outgoing.send((epoch, Err("offline".into()))).unwrap();
    logs.update(&ctx, "http://unused", false, true);
    assert_eq!(logs.error, "offline");
    assert_eq!(
        logs.entries,
        vec![entry(50)],
        "retain last successful logs on failure"
    );
    logs.update(&ctx, "http://unused", false, false);
    assert!(incoming.try_recv().is_err());
    logs.update(&ctx, "http://unused", false, true);
    incoming.try_recv().unwrap();
    outgoing.send((epoch, Ok(vec![entry(1)]))).unwrap();
    logs.update(&ctx, "http://unused", false, true);
    assert_eq!(
        logs.entries,
        vec![entry(1)],
        "restarted sequence replaces old server window"
    );
    assert!(logs.error.is_empty());
    logs.resume();
    logs.update(&ctx, "http://unused", false, true);
    incoming.try_recv().unwrap();
    outgoing.send((epoch, Ok(vec![]))).unwrap();
    logs.update(&ctx, "http://unused", false, true);
    assert!(logs.entries.is_empty());
    drop(logs);
    assert!(outgoing.send((epoch, Ok(vec![entry(2)]))).is_err());
}

#[test]
fn timestamps_and_copy_preserve_unicode_and_complete_lines() {
    let mut row = entry(1);
    row.time = 86_399_999;
    row.line = "WARN artwork: café / 曲\nsecond line".into();
    assert_eq!(row.timestamp(), "23:59:59");
    let mut logs = Logs {
        entries: vec![row],
        ..Default::default()
    };
    assert_eq!(
        logs.copy_text(),
        "23:59:59 UTC  WARN artwork: café / 曲\nsecond line"
    );
    logs.entries[0].time = 86_400_000;
    assert_eq!(logs.entries[0].timestamp(), "00:00:00");
}
