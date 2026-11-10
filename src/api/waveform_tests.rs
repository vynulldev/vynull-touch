// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

#[test]
fn optional_three_band_analysis_validates_identity_encoding_and_session() {
    for (body, status, session, valid) in [
        (
            r#"{"track_id":7,"style":"3band","sample_rate":150,"entries":[{"bass":255,"mid":100,"high":20}]}"#,
            "200 OK",
            "one",
            true,
        ),
        (
            r#"{"track_id":8,"style":"3band","sample_rate":150,"entries":[{"bass":255,"mid":100,"high":20}]}"#,
            "200 OK",
            "one",
            false,
        ),
        (
            r#"{"track_id":7,"style":"3band","sample_rate":0,"entries":null}"#,
            "200 OK",
            "one",
            false,
        ),
        (
            r#"{"track_id":7,"style":"spectral","sample_rate":150,"entries":[{"r":7,"g":3,"b":1,"h":20}]}"#,
            "200 OK",
            "one",
            false,
        ),
        (
            r#"{"track_id":7,"style":"3band","sample_rate":150,"entries":null}"#,
            "200 OK",
            "one",
            false,
        ),
        ("{}", "404 Not Found", "one", false),
        ("{}", "200 OK", "two", false),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        api.session = Some("one".into());
        let task = thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
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
            assert!(request.starts_with("get /api/analysis/waveform/7?type=detail&style=3band "));
            assert!(request.contains("x-vynull-library-session: one"));
            write!(socket, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
        });
        let result = api.waveform_bands(7);
        task.join().unwrap();
        if session != "one" {
            assert!(result.unwrap_err().starts_with("Library changed"));
        } else {
            let bands = result.unwrap();
            assert_eq!(bands.available(), valid);
            if valid {
                assert_eq!(bands.entries[0].bass, 255);
            }
        }
    }
}
