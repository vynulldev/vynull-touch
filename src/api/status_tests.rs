// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::{io::Write, net::TcpListener};

#[test]
fn status_timestamp_uses_status_round_trip_and_excludes_slow_link_request() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let mut status_sent = Instant::now();
        for (path, delay, body) in [
            ("/api/status", 100, r#"{"peers":[],"players":[]}"#),
            ("/api/link", 180, r#"{"linked":true}"#),
        ] {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = vec![];
            let mut byte = [0];
            while !data.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).unwrap();
                data.push(byte[0]);
            }
            assert_eq!(
                String::from_utf8(data).unwrap().split_whitespace().nth(1),
                Some(path)
            );
            thread::sleep(Duration::from_millis(delay));
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
            if path == "/api/status" {
                status_sent = Instant::now();
            }
        }
        status_sent
    });
    let api = Api::new(&origin).unwrap();
    let started = Instant::now();
    let (sampled, (_, linked)) = api.sample_status().unwrap();
    let status_sent = server.join().unwrap();
    assert!(linked);
    assert!(
        sampled >= started + Duration::from_millis(40),
        "must compensate status round trip"
    );
    assert!(
        sampled <= status_sent,
        "slow link request must not shift the status timestamp"
    );
}

#[test]
fn player_status_keeps_polling_while_waveform_analysis_is_blocked() {
    for background in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let stopped = Arc::new(AtomicBool::new(false));
        let stalled = Arc::new(AtomicBool::new(false));
        let released = Arc::new(AtomicBool::new(false));
        let (stop, stall, release) = (stopped.clone(), stalled.clone(), released.clone());
        let server = thread::spawn(move || {
            let mut handlers = vec![];
            while !stop.load(Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => panic!("{e}"),
                };
                let (stall, release) = (stall.clone(), release.clone());
                handlers.push(thread::spawn(move || {
                socket.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
                let mut data=vec![];let mut b=[0];
                while !data.ends_with(b"\r\n\r\n") {socket.read_exact(&mut b).unwrap();data.push(b[0]);}
                let text=String::from_utf8(data).unwrap();let path=text.split_whitespace().nth(1).unwrap();
                let body=match path {
                    "/api/tracks/rev"=>r#"{"library_session":"one"}"#,
                    "/api/tracks"=>r#"[{"id":1}]"#,
                    "/api/cues/rev"=>"{}",
                    "/api/playlists"|"/api/playlists/9/tracks"|"/api/tracks/1/cues"|"/api/tracks/1/phrases"=>"[]",
                    "/api/status"=>r#"{"peers":[],"players":[]}"#,
                    "/api/link"=>r#"{"linked":true}"#,
                    "/api/analysis/1"=>r#"{"track_id":1,"beats":[]}"#,
                    p if p.starts_with("/api/analysis/waveform/")=>{
                        stall.store(true,Ordering::Relaxed);
                        let deadline=Instant::now()+Duration::from_secs(3);
                        while !release.load(Ordering::Relaxed)&&Instant::now()<deadline {thread::sleep(Duration::from_millis(5));}
                        r#"{"track_id":1,"sample_rate":150,"entries":[]}"#
                    }
                    _=>panic!("unexpected request {path}"),
                };
                let _=write!(socket,"HTTP/1.1 200 OK\r\nX-Vynull-Library-Session: one\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len());
            }));
            }
            for task in handlers {
                task.join().unwrap();
            }
        });
        let worker = Worker::spawn(origin, egui::Context::default());
        assert!(matches!(
            worker.events.recv_timeout(Duration::from_secs(3)).unwrap(),
            Event::Library(Ok(_))
        ));
        let started = Instant::now();
        if background {
            worker
                .refresh
                .send(refresh::Request {
                    force: true,
                    detail: Some(1),
                    ..Default::default()
                })
                .unwrap();
        } else {
            worker.commands.send(Command::Detail(1, 1)).unwrap();
        }
        let mut observed = false;
        while started.elapsed() < Duration::from_millis(1200) {
            if let Ok(Event::Status(sampled, Ok(_))) =
                worker.events.recv_timeout(Duration::from_millis(100))
                && stalled.load(Ordering::Relaxed)
                && sampled > started
            {
                observed = true;
                break;
            }
        }
        let mut command_observed = !background;
        if background {
            worker.commands.send(Command::Playlist(9, 2)).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            while Instant::now() < deadline {
                if let Ok(Event::Playlist(9, 2, Ok(_))) =
                    worker.events.recv_timeout(Duration::from_millis(100))
                {
                    command_observed = true;
                    break;
                }
            }
        }
        released.store(true, Ordering::Relaxed);
        drop(worker);
        stopped.store(true, Ordering::Relaxed);
        server.join().unwrap();
        assert!(observed, "status was blocked by the analysis request");
        assert!(
            command_observed,
            "foreground request was blocked by background analysis"
        );
    }
}
