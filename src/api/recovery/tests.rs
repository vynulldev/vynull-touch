// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener, sync::atomic::AtomicUsize};

fn receive(worker: &Worker, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let event = worker
            .events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .expect("connection event before timeout");
        if predicate(&event) {
            return event;
        }
    }
}

#[test]
fn retry_waits_grow_to_thirty_seconds_and_cancel_promptly() {
    let delays: Vec<_> = (1..=8).map(|attempt| backoff(attempt).as_secs()).collect();
    assert_eq!(delays, [1, 2, 4, 8, 16, 30, 30, 30]);
    assert_eq!(backoff(u32::MAX), Duration::from_secs(30));
    let cancelled = AtomicBool::new(true);
    let retry = AtomicBool::new(false);
    assert!(!wait(
        &cancelled,
        &retry,
        Instant::now() + Duration::from_secs(30)
    ));
    cancelled.store(false, Ordering::Relaxed);
    retry.store(true, Ordering::Relaxed);
    assert!(wait(
        &cancelled,
        &retry,
        Instant::now() + Duration::from_secs(30)
    ));
    assert!(!retry.load(Ordering::Relaxed));
}

#[test]
fn initial_retry_manual_wakeup_outage_recovery_and_restart_never_repeat_a_cue_write() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    // 0: unavailable (proxy error without session header), 1: live, 2: new session.
    let mode = Arc::new(AtomicUsize::new(0));
    let posts = Arc::new(AtomicUsize::new(0));
    let revisions = Arc::new(AtomicUsize::new(0));
    let stopped = Arc::new(AtomicBool::new(false));
    let (server_mode, server_posts, server_revisions, stop) = (
        mode.clone(),
        posts.clone(),
        revisions.clone(),
        stopped.clone(),
    );
    let server = thread::spawn(move || {
        let mut handlers = vec![];
        let deadline = Instant::now() + Duration::from_secs(20);
        while !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            let (mode, posts, revisions) = (
                server_mode.clone(),
                server_posts.clone(),
                server_revisions.clone(),
            );
            handlers.push(thread::spawn(move || {
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut bytes = vec![];
                let mut byte = [0];
                while !bytes.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                }
                let request = String::from_utf8(bytes).unwrap();
                let mut parts = request.split_whitespace();
                let method = parts.next().unwrap();
                let path = parts.next().unwrap();
                if let Some(length) = request.lines().find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").and_then(|n| n.trim().parse::<usize>().ok())) {
                    socket.read_exact(&mut vec![0; length]).unwrap();
                }
                if path == "/api/tracks/rev" { revisions.fetch_add(1, Ordering::Relaxed); }
                if method == "POST" { posts.fetch_add(1, Ordering::Relaxed); }
                let mode = mode.load(Ordering::Relaxed);
                let (status, body) = if mode == 0 || method == "POST" {
                    ("503 Service Unavailable", r#"{"error":"temporarily unavailable"}"#.to_owned())
                } else {
                    ("200 OK", match path {
                        "/api/tracks/rev" => format!(r#"{{"library_session":"{}"}}"#, if mode == 2 { "two" } else { "one" }),
                        "/api/tracks" | "/api/playlists" | "/api/tracks/1/cues" => "[]".into(),
                        "/api/status" => r#"{"peers":[],"players":[]}"#.into(),
                        "/api/link" => r#"{"linked":true}"#.into(),
                        "/api/tracks/1" => r#"{"id":1,"file_path":"/music/one.flac"}"#.into(),
                        _ => panic!("unexpected {method} {path}"),
                    })
                };
                let session = if mode == 0 { String::new() } else { format!("X-Vynull-Library-Session: {}\r\n", if mode == 2 { "two" } else { "one" }) };
                let _ = write!(socket, "HTTP/1.1 {status}\r\n{session}Content-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
            }));
        }
        for handler in handlers {
            handler.join().unwrap();
        }
    });
    let worker = Worker::spawn(origin.clone(), egui::Context::default());
    receive(&worker, |event| matches!(event, Event::Library(Err(_))));
    receive(&worker, |event| {
        matches!(
            event,
            Event::Connection(Connection::Waiting { attempt: 1, .. })
        )
    });
    thread::sleep(Duration::from_millis(100));
    assert_eq!(
        revisions.load(Ordering::Relaxed),
        1,
        "failed initial reads must wait, not spin"
    );
    mode.store(1, Ordering::Relaxed);
    worker.retry();
    receive(
        &worker,
        |event| matches!(event, Event::Library(Ok(library)) if library.session.as_deref() == Some("one")),
    );
    receive(&worker, |event| matches!(event, Event::Status(_, Ok(_))));
    worker
        .commands
        .send(Command::Cues {
            track: Box::new(Track {
                id: 1,
                file_path: "/music/one.flac".into(),
                ..Default::default()
            }),
            token: 1,
            expected: None,
            change: Some(Change::Put(Cue {
                number: 1,
                time_ms: 1000,
                ..Default::default()
            })),
        })
        .unwrap();
    receive(
        &worker,
        |event| matches!(event, Event::Cues(_, _, true, Err(error)) if error.contains("Save not confirmed")),
    );
    assert_eq!(posts.load(Ordering::Relaxed), 1);

    mode.store(0, Ordering::Relaxed);
    receive(&worker, |event| {
        matches!(
            event,
            Event::Connection(Connection::Waiting { attempt: 1, .. })
        )
    });
    receive(&worker, |event| {
        matches!(
            event,
            Event::Connection(Connection::Waiting { attempt: 2, .. })
        )
    });
    mode.store(1, Ordering::Relaxed);
    // This recovery is automatic; only the earlier initial connection used Retry.
    receive(&worker, |event| matches!(event, Event::Status(_, Ok(_))));
    assert_eq!(posts.load(Ordering::Relaxed), 1);
    assert_eq!(
        revisions.load(Ordering::Relaxed),
        2,
        "same-session recovery must keep the library"
    );

    mode.store(2, Ordering::Relaxed);
    receive(
        &worker,
        |event| matches!(event, Event::Status(_, Err(error)) if error.starts_with("Library changed")),
    );
    drop(worker);
    let replacement = Worker::spawn(origin, egui::Context::default());
    receive(
        &replacement,
        |event| matches!(event, Event::Library(Ok(library)) if library.session.as_deref() == Some("two")),
    );
    receive(&replacement, |event| {
        matches!(event, Event::Status(_, Ok(_)))
    });
    assert_eq!(posts.load(Ordering::Relaxed), 1);
    drop(replacement);
    stopped.store(true, Ordering::Relaxed);
    server.join().unwrap();
}
