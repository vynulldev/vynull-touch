// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

#[test]
fn phrase_reads_are_session_bound_optional_and_do_not_discard_other_analysis() {
    for (status, session, body, available, warning) in [
        (
            "200 OK",
            "one",
            r#"[{"kind":1,"name":"INTRO","start_ms":125,"end_ms":32125}]"#,
            true,
            false,
        ),
        ("200 OK", "one", "null", false, false),
        ("200 OK", "one", "[]", false, false),
        ("404 Not Found", "one", "{}", false, false),
        ("405 Method Not Allowed", "one", "{}", false, false),
        ("501 Not Implemented", "one", "{}", false, false),
        ("503 Service Unavailable", "one", "{}", false, true),
        ("200 OK", "one", "{}", false, true),
        (
            "200 OK",
            "one",
            r#"[{"kind":1,"start_ms":1000,"end_ms":500}]"#,
            false,
            true,
        ),
        ("200 OK", "two", "[]", false, false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        api.session = Some("one".into());
        let server = thread::spawn(move || {
            for (path, status, session, body) in [
                ("/api/tracks/7/cues", "200 OK", "one", "[]"),
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
                    r#"{"track_id":7,"beats":[{"time_ms":125,"beat_in_bar":1}]}"#,
                ),
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
        let result = api.detail(7);
        server.join().unwrap();
        if session != "one" {
            assert!(result.err().unwrap().starts_with("Library changed"));
        } else {
            let detail = result.unwrap();
            assert_eq!(!detail.phrases.is_empty(), available);
            assert_eq!(
                detail.warning.contains("Phrase sections unavailable"),
                warning
            );
            assert_eq!(detail.waveform.entries.len(), 1);
            assert!(!detail.beat_grid.is_empty());
            assert!(detail.cues_ready);
        }
    }
}
