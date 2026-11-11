// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    beat_grid::{BeatGrid, TrackAnalysis},
    cues::{Change, Saved},
    model::*,
};
use eframe::egui;
use reqwest::{Method, Url, blocking::Client};
use serde::{Deserialize, de::DeserializeOwned};
use std::{
    io::Read,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};
mod browser_settings;
pub(crate) mod deck_assets;
mod device_settings;
mod editing;
mod grid;
pub(crate) mod playlist_counts;
mod recovery;
pub(crate) mod refresh;
pub use recovery::Connection;

const SESSION: &str = "X-Vynull-Library-Session";
const LIMIT: u64 = 64 * 1024 * 1024;
#[cfg(test)]
mod cue_tests;
#[cfg(test)]
mod phrase_tests;
#[cfg(test)]
mod status_tests;
#[cfg(test)]
mod waveform_tests;

pub fn validate_origin(origin: &str) -> Result<Url, String> {
    let url =
        Url::parse(origin).map_err(|_| "Use an HTTP(S) origin, e.g. http://127.0.0.1:9443")?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Use an HTTP(S) origin without a path, query or credentials".into());
    }
    Ok(url)
}

pub enum Command {
    BrowserSettings(u64, Option<Box<crate::browser_settings::Edit>>),
    DeviceSettings(u64, Option<Vec<crate::device_settings::Change>>),
    GridRestore {
        track: Box<Track>,
        token: u64,
        expected: Box<crate::grid_edit::Snapshot>,
        target: Box<crate::grid_edit::Snapshot>,
    },
    Grid {
        track: Box<Track>,
        token: u64,
        expected: Option<Box<crate::grid_edit::Snapshot>>,
        change: Option<crate::grid_edit::Change>,
    },
    Edit(u64, Box<crate::editing::Request>),
    Playlist(u32, u64),
    Detail(u32, u64),
    Load {
        track: Box<Track>,
        peer: Peer,
        confirmed: Option<Box<Player>>,
    },
    Cues {
        track: Box<Track>,
        token: u64,
        expected: Option<Cue>,
        change: Option<Change>,
    },
}

pub struct LibrarySnapshot {
    pub tracks: Vec<Track>,
    pub playlists: Vec<Playlist>,
    pub session: Option<String>,
    pub revision: Option<u64>,
}

pub enum Event {
    BrowserSettings(u64, bool, Result<crate::browser_settings::Snapshot, String>),
    DeviceSettings(u64, bool, Result<crate::device_settings::Snapshot, String>),
    Grid(
        u32,
        u64,
        bool,
        Result<Box<crate::grid_edit::Snapshot>, String>,
    ),
    Edit(u64, Result<crate::editing::Reply, String>),
    Connection(Connection),
    Library(Result<LibrarySnapshot, String>),
    Playlist(u32, u64, Result<Vec<Track>, String>),
    Detail(u32, u64, Result<Detail, String>),
    Status(Instant, Result<(Status, bool), String>),
    Load(Result<(u32, u8), String>),
    Cues(u32, u64, bool, Result<Saved, String>),
    Refresh(refresh::Request, Result<Box<refresh::Snapshot>, String>),
}

pub struct Worker {
    pub commands: SyncSender<Command>,
    pub events: Receiver<Event>,
    pub refresh: SyncSender<refresh::Request>,
    cancelled: Arc<AtomicBool>,
    retry: Arc<AtomicBool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Worker {
    #[cfg(test)]
    pub(crate) fn test() -> (Self, Receiver<Command>, mpsc::Sender<Event>) {
        let (commands, requests) = mpsc::sync_channel(8);
        let (responses, events) = mpsc::channel();
        let (refresh, _) = mpsc::sync_channel(1);
        (
            Self {
                commands,
                events,
                refresh,
                cancelled: Arc::new(AtomicBool::new(false)),
                retry: Arc::new(AtomicBool::new(false)),
            },
            requests,
            responses,
        )
    }

    pub fn retry(&self) {
        self.retry.store(true, Ordering::Relaxed);
    }

    pub fn spawn(origin: String, ctx: egui::Context) -> Self {
        let (commands, requests) = mpsc::sync_channel(8);
        let (responses, events) = mpsc::channel();
        let (refresh, refresh_requests) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let worker_cancelled = cancelled.clone();
        let retry = Arc::new(AtomicBool::new(false));
        let worker_retry = retry.clone();
        thread::spawn(move || {
            let mut api = match Api::new(&origin) {
                Ok(api) => api,
                Err(error) => {
                    let _ = responses.send(Event::Library(Err(error)));
                    ctx.request_repaint();
                    return;
                }
            };
            api.cancelled = worker_cancelled;
            let send = |event| {
                let result = responses.send(event);
                ctx.request_repaint();
                result.is_ok()
            };
            let mut failures = 0_u32;
            loop {
                let result = api.library();
                let ready = result.is_ok();
                let changed = result
                    .as_ref()
                    .err()
                    .is_some_and(|error| error.starts_with("Library changed"));
                if !send(Event::Library(result)) || changed || api.cancelled.load(Ordering::Relaxed)
                {
                    return;
                }
                if ready {
                    break;
                }
                failures = failures.saturating_add(1);
                let until = Instant::now() + recovery::backoff(failures);
                if !send(Event::Connection(Connection::Waiting {
                    attempt: failures,
                    until,
                })) || !recovery::wait(&api.cancelled, &worker_retry, until)
                    || !send(Event::Connection(Connection::Connecting {
                        attempt: failures,
                    }))
                {
                    return;
                }
            }
            // Status has its own HTTP client and worker: waveform analysis and
            // cue writes must not stall the deck monitor or its load guards.
            let status_sender = responses.clone();
            let status_ctx = ctx.clone();
            let status_cancelled = api.cancelled.clone();
            let session = api.session.clone();
            let status_retry = worker_retry;
            refresh::spawn(
                origin.clone(),
                session.clone(),
                api.cancelled.clone(),
                refresh_requests,
                responses.clone(),
                ctx.clone(),
            );
            thread::spawn(move || {
                let Ok(mut api) = Api::new(&origin) else {
                    return;
                };
                api.session = session;
                api.cancelled = status_cancelled;
                let mut failures = 0_u32;
                while !api.cancelled.load(Ordering::Relaxed) {
                    let (sampled, result) = match api.sample_status() {
                        Ok((sampled, status)) => (sampled, Ok(status)),
                        Err(error) => (Instant::now(), Err(error)),
                    };
                    let failed = result.is_err();
                    let changed = result
                        .as_ref()
                        .err()
                        .is_some_and(|error| error.starts_with("Library changed"));
                    if status_sender.send(Event::Status(sampled, result)).is_err() {
                        break;
                    }
                    status_ctx.request_repaint();
                    if changed {
                        return;
                    }
                    failures = if failed {
                        failures.saturating_add(1)
                    } else {
                        0
                    };
                    let until = Instant::now()
                        + if failed {
                            recovery::backoff(failures)
                        } else {
                            Duration::from_millis(250)
                        };
                    if failed {
                        let _ = status_sender.send(Event::Connection(Connection::Waiting {
                            attempt: failures,
                            until,
                        }));
                        status_ctx.request_repaint();
                    }
                    if !recovery::wait(&api.cancelled, &status_retry, until) {
                        return;
                    }
                    if failed {
                        let _ = status_sender.send(Event::Connection(Connection::Connecting {
                            attempt: failures,
                        }));
                        status_ctx.request_repaint();
                    }
                }
            });
            loop {
                if api.cancelled.load(Ordering::Relaxed) {
                    break;
                }
                match requests.recv_timeout(Duration::from_millis(100)) {
                    Ok(command) => {
                        let event = match command {
                            Command::BrowserSettings(token, edit) => Event::BrowserSettings(
                                token,
                                edit.is_some(),
                                match edit {
                                    Some(edit) => api.save_browser_settings(&edit),
                                    None => api.browser_settings(),
                                },
                            ),
                            Command::DeviceSettings(token, changes) => Event::DeviceSettings(
                                token,
                                changes.is_some(),
                                match changes {
                                    Some(changes) => api.save_device_settings(&changes),
                                    None => api.device_settings(),
                                },
                            ),
                            Command::GridRestore {
                                track,
                                token,
                                expected,
                                target,
                            } => Event::Grid(
                                track.id,
                                token,
                                true,
                                api.restore_grid(&track, &expected, &target),
                            ),
                            Command::Grid {
                                track,
                                token,
                                expected,
                                change,
                            } => Event::Grid(
                                track.id,
                                token,
                                change.is_some(),
                                api.edit_grid(&track, expected.as_deref(), change),
                            ),
                            Command::Edit(token, request) => Event::Edit(token, api.edit(&request)),
                            Command::Playlist(id, token) => Event::Playlist(
                                id,
                                token,
                                api.list(&format!("api/playlists/{id}/tracks")),
                            ),
                            Command::Detail(id, token) => Event::Detail(id, token, api.detail(id)),
                            Command::Load {
                                track,
                                peer,
                                confirmed,
                            } => Event::Load(api.load(&track, &peer, confirmed.as_deref())),
                            Command::Cues {
                                track,
                                token,
                                expected,
                                change,
                            } => Event::Cues(
                                track.id,
                                token,
                                change.is_some(),
                                api.edit_cue(&track, expected.as_ref(), change.as_ref()),
                            ),
                        };
                        if !send(event) {
                            break;
                        }
                    }
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => (),
                }
            }
        });
        Self {
            commands,
            events,
            refresh,
            cancelled,
            retry,
        }
    }
}

struct Api {
    http: Client,
    origin: Url,
    session: Option<String>,
    cancelled: Arc<AtomicBool>,
}

#[derive(Deserialize)]
struct Revision {
    library_session: Option<String>,
    rev: Option<u64>,
}
#[derive(Deserialize)]
struct Link {
    linked: bool,
}

impl Api {
    fn new(origin: &str) -> Result<Self, String> {
        Ok(Self {
            http: Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()
                .map_err(|e| e.to_string())?,
            origin: validate_origin(origin)?,
            session: None,
            cancelled: Arc::new(AtomicBool::new(false)),
        })
    }

    fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<serde_json::Value>,
        timeout: u64,
    ) -> Result<T, String> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err("Connection closed; request cancelled".into());
        }
        let mut request = self
            .http
            .request(method, self.origin.join(path).map_err(|e| e.to_string())?)
            .timeout(Duration::from_secs(timeout));
        if let Some(session) = &self.session {
            request = request.header(SESSION, session);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .map_err(|e| format!("Cannot reach Vynull: {e}"))?;
        let status = response.status();
        let observed = response
            .headers()
            .get(SESSION)
            .and_then(|v| v.to_str().ok());
        // A proxy's error response may omit daemon headers. That is a failed
        // request, not evidence of a different library. Successful reads still
        // require the expected session, and explicit mismatches fail closed.
        if let Some(session) = &self.session
            && observed != Some(session.as_str())
            && (status.is_success()
                || observed.is_some()
                || status == reqwest::StatusCode::CONFLICT)
        {
            return Err("Library changed; reconnect to reload its tracks".into());
        }
        let mut bytes = Vec::new();
        response
            .take(if status.is_success() { LIMIT + 1 } else { 4096 })
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if !status.is_success() {
            let message = serde_json::from_slice::<serde_json::Value>(&bytes)
                .ok()
                .and_then(|v| v["error"].as_str().map(str::to_owned))
                .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned());
            return Err(format!("{path}: HTTP {} — {message}", status.as_u16()));
        }
        if bytes.len() as u64 > LIMIT {
            return Err("Response exceeds the 64 MiB limit".into());
        }
        serde_json::from_slice(&bytes).map_err(|e| format!("Invalid Vynull response: {e}"))
    }

    fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, String> {
        self.json(Method::GET, path, None, 20)
    }
    fn list<T: DeserializeOwned>(&self, path: &str) -> Result<Vec<T>, String> {
        Ok(self.get::<Option<Vec<T>>>(path)?.unwrap_or_default())
    }

    fn library(&mut self) -> Result<LibrarySnapshot, String> {
        // Do not silently rebind IDs if a library is switched under the UI.
        let revision: Revision = self.get("api/tracks/rev")?;
        let session = revision.library_session.filter(|s| !s.is_empty());
        if self.session.is_some() && self.session != session {
            return Err("Library changed; reconnect to reload its tracks".into());
        }
        self.session = session;
        Ok(LibrarySnapshot {
            tracks: self.list("api/tracks")?,
            playlists: self.list("api/playlists")?,
            session: self.session.clone(),
            revision: revision.rev,
        })
    }

    fn status(&self) -> Result<(Status, bool), String> {
        self.sample_status().map(|(_, status)| status)
    }

    fn sample_status(&self) -> Result<(Instant, (Status, bool)), String> {
        let started = Instant::now();
        let status = self.json(Method::GET, "api/status", None, 3)?;
        // Estimate when the server sampled the status. A separate link request
        // must not make that observation appear newer (or add a full RTT).
        let sampled = started + started.elapsed() / 2;
        let link: Link = self.json(Method::GET, "api/link", None, 3)?;
        Ok((sampled, (status, link.linked)))
    }

    fn detail(&self, id: u32) -> Result<Detail, String> {
        let mut detail = Detail::default();
        match self.list(&format!("api/tracks/{id}/cues")) {
            Ok(cues) => {
                detail.cues = cues;
                detail.cues_ready = true;
            }
            Err(e) if e.starts_with("Library changed") => return Err(e),
            Err(e) => detail.warning = format!("Cues unavailable: {e}"),
        }
        match self.json::<Waveform>(
            Method::GET,
            &format!("api/analysis/waveform/{id}?type=detail&style=spectral"),
            None,
            60,
        ) {
            Ok(wave) if wave.track_id == id && wave.sample_rate > 0 => detail.waveform = wave,
            Ok(_) => detail
                .warning
                .push_str(" Waveform identity or sample rate is invalid."),
            Err(e) if e.starts_with("Library changed") => return Err(e),
            Err(e) => detail
                .warning
                .push_str(&format!(" Waveform unavailable: {e}")),
        }
        match self
            .get::<TrackAnalysis>(&format!("api/analysis/{id}"))
            .and_then(|analysis| BeatGrid::new(analysis, id).map_err(str::to_owned))
        {
            Ok(grid) => detail.beat_grid = grid,
            Err(e) if e.starts_with("Library changed") => return Err(e),
            Err(e) => detail
                .warning
                .push_str(&format!(" Beat grid unavailable: {e}")),
        }
        detail.bands = self.waveform_bands(id)?;
        match self.phrase_sections(id) {
            Ok(phrases) => detail.phrases = phrases,
            Err(e) if e.starts_with("Library changed") => return Err(e),
            Err(e) => detail
                .warning
                .push_str(&format!(" Phrase sections unavailable: {e}")),
        }
        Ok(detail)
    }

    fn phrase_sections(&self, id: u32) -> Result<crate::phrases::Sections, String> {
        match self.json::<Option<Vec<crate::phrases::Phrase>>>(
            Method::GET,
            &format!("api/tracks/{id}/phrases"),
            None,
            5,
        ) {
            Ok(phrases) => {
                crate::phrases::Sections::new(phrases.unwrap_or_default()).map_err(str::to_owned)
            }
            Err(e)
                if ["HTTP 404", "HTTP 405", "HTTP 501"]
                    .iter()
                    .any(|s| e.contains(s)) =>
            {
                Ok(Default::default())
            }
            Err(e) => Err(e),
        }
    }

    fn waveform_bands(&self, id: u32) -> Result<crate::waveform::Bands, String> {
        // Older servers may only supply spectral data. Reject mismatched
        // identities/encodings, and propagate a changed library session.
        match self.json::<crate::waveform::Bands>(
            Method::GET,
            &format!("api/analysis/waveform/{id}?type=detail&style=3band"),
            None,
            5,
        ) {
            Ok(bands)
                if bands.track_id == id && bands.style == "3band" && bands.sample_rate > 0 =>
            {
                Ok(bands)
            }
            Err(e) if e.starts_with("Library changed") => Err(e),
            _ => Ok(Default::default()),
        }
    }

    fn edit_cue(
        &self,
        expected_track: &Track,
        expected: Option<&Cue>,
        change: Option<&Change>,
    ) -> Result<Saved, String> {
        if self.session.is_none() {
            return Err(
                "Reconnect to a daemon with library-session support before editing cues".into(),
            );
        }
        let id = expected_track.id;
        let track: Track = self.get(&format!("api/tracks/{id}"))?;
        if id == 0
            || track.id != id
            || track.file_path.is_empty()
            || track.file_path != expected_track.file_path
        {
            return Err("Library changed; reconnect before editing cues".into());
        }
        let path = format!("api/tracks/{id}/cues");
        let before: Vec<Cue> = self.list(&path)?;
        let Some(change) = change else {
            return Ok(Saved {
                cues: before,
                persisted: false,
            });
        };
        if change.number() == 0 {
            return Err("Invalid cue slot".into());
        }
        let actual = before.iter().find(|c| c.number == change.number());
        if actual != expected && actual != change.desired() {
            return Err(
                "This cue changed in another client. Reload cues before editing it again.".into(),
            );
        }
        #[derive(Deserialize)]
        struct Receipt {
            ok: bool,
            #[serde(default)]
            persisted: bool,
        }
        let (method, path, body) = match change {
            Change::Put(cue) => (
                Method::POST,
                path,
                Some(serde_json::to_value(cue).map_err(|e| e.to_string())?),
            ),
            Change::Delete(number) => (Method::DELETE, format!("{path}/{number}"), None),
        };
        // One mutation only. A lost response may still have changed the daemon.
        let receipt: Receipt = self
            .json(method, &path, body, 15)
            .map_err(|e| format!("Save not confirmed; reload cues or retry explicitly. {e}"))?;
        if !receipt.ok {
            return Err("Daemon did not accept the cue edit".into());
        }
        let cues: Vec<Cue> = self.list(&format!("api/tracks/{id}/cues")).map_err(|e| {
            format!("Edit accepted, but readback failed. Reload cues before continuing. {e}")
        })?;
        if cues.iter().find(|c| c.number == change.number()) != change.desired() {
            return Err("Cue readback differs from the edit. Reload cues to review it; the daemon may not support cue labels.".into());
        }
        Ok(Saved {
            cues,
            persisted: receipt.persisted,
        })
    }

    fn load(
        &self,
        expected: &Track,
        target: &Peer,
        confirmed: Option<&Player>,
    ) -> Result<(u32, u8), String> {
        if self.session.is_none() {
            return Err(
                "This daemon does not expose a library session; update it before loading players"
                    .into(),
            );
        }
        let track: Track = self.get(&format!("api/tracks/{}", expected.id))?;
        if track.id == 0
            || track.id != expected.id
            || track.file_missing
            || track.file_path.is_empty()
            || track.file_path != expected.file_path
        {
            return Err("Track changed or its file is unavailable; refresh the collection".into());
        }
        let (status, linked) = self.status()?;
        if !linked {
            return Err("Enable Pro DJ Link in Vynull first".into());
        }
        if let Some(error) = status.load_confirmation_error(target, confirmed) {
            return Err(error.into());
        }
        #[derive(Deserialize)]
        struct Receipt {
            ok: bool,
            track_id: u32,
            device_number: u8,
        }
        // Never retry this mutation. A lost response can still mean a sent command.
        let receipt: Receipt = self.json(Method::POST, "api/load", Some(serde_json::json!({"track_id": track.id, "device_number": target.device_number})), 3)
            .map_err(|e| format!("Load outcome unknown; check the player. {e}"))?;
        if !receipt.ok
            || receipt.track_id != track.id
            || receipt.device_number != target.device_number
        {
            return Err(
                "Load outcome unknown: acknowledgement did not match; check the player".into(),
            );
        }
        Ok((track.id, target.device_number))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, net::TcpListener};
    fn fixture(headers: &str, body: &str) -> (Api, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let reply = format!(
            "HTTP/1.1 200 OK\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = vec![];
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8(request).unwrap().to_lowercase()
        });
        (api, handle)
    }
    #[test]
    fn null_list_and_session_headers_match_go_contract() {
        let (mut api, task) = fixture("X-Vynull-Library-Session: one\r\n", "null");
        api.session = Some("one".into());
        assert!(api.list::<Track>("api/tracks").unwrap().is_empty());
        assert!(
            task.join()
                .unwrap()
                .contains("x-vynull-library-session: one")
        );
    }
    #[test]
    fn rejects_data_from_changed_library() {
        let (mut api, task) = fixture("X-Vynull-Library-Session: two\r\n", "[]");
        api.session = Some("one".into());
        assert!(
            api.list::<Track>("api/tracks")
                .unwrap_err()
                .starts_with("Library changed")
        );
        task.join().unwrap();
    }
    #[test]
    fn validates_server_origin() {
        assert!(validate_origin("http://pi.local:9443").is_ok());
        for bad in [
            "file:///tmp/test",
            "http://host/api",
            "http://host?q=x",
            "http://user:pass@host",
            "http://host/#hash",
        ] {
            assert!(validate_origin(bad).is_err());
        }
    }

    #[test]
    fn detail_fetches_the_saved_grid_without_losing_waveforms_on_grid_errors() {
        for case in [
            "grid",
            "missing",
            "unavailable",
            "wrong track",
            "changed library",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            api.session = Some("one".into());
            let grid = match case {
                "missing" => r#"{"track_id":1,"beats":null}"#,
                "wrong track" => r#"{"track_id":2,"beats":[]}"#,
                _ => {
                    r#"{"track_id":1,"beats":[{"time_ms":125.5,"beat_in_bar":3},{"time_ms":625.5,"beat_in_bar":4},{"time_ms":1125.5,"beat_in_bar":1}]}"#
                }
            };
            let task = thread::spawn(move || {
                let mut requests = vec![];
                let mut replies = vec![
                    ("200 OK", "one", "[]"),
                    (
                        "200 OK",
                        "one",
                        r#"{"track_id":1,"sample_rate":150,"entries":[{"r":1,"g":2,"b":3,"h":24}]}"#,
                    ),
                    (
                        if case == "unavailable" {
                            "404 Not Found"
                        } else {
                            "200 OK"
                        },
                        if case == "changed library" {
                            "two"
                        } else {
                            "one"
                        },
                        grid,
                    ),
                ];
                if case != "changed library" {
                    replies.extend([("404 Not Found", "one", "{}"), ("200 OK", "one", "[]")]);
                }
                for (status, session, body) in replies {
                    let deadline = Instant::now() + Duration::from_secs(3);
                    let mut stream = loop {
                        match listener.accept() {
                            Ok((stream, _)) => break stream,
                            Err(e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    && Instant::now() < deadline =>
                            {
                                thread::sleep(Duration::from_millis(10));
                            }
                            Err(e) => panic!("Expected another detail request: {e}"),
                        }
                    };
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = vec![];
                    let mut byte = [0];
                    while !request.ends_with(b"\r\n\r\n") {
                        stream.read_exact(&mut byte).unwrap();
                        request.push(byte[0]);
                    }
                    requests.push(String::from_utf8(request).unwrap().to_lowercase());
                    write!(stream, "HTTP/1.1 {status}\r\nX-Vynull-Library-Session: {session}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
                requests
            });
            let result = api.detail(1);
            let requests = task.join().unwrap();
            assert!(requests[0].starts_with("get /api/tracks/1/cues "));
            assert!(
                requests[1].starts_with("get /api/analysis/waveform/1?type=detail&style=spectral ")
            );
            assert!(requests[2].starts_with("get /api/analysis/1 "));
            assert!(
                requests
                    .iter()
                    .all(|r| r.contains("x-vynull-library-session: one"))
            );
            if case == "changed library" {
                assert!(result.err().unwrap().starts_with("Library changed"));
                continue;
            }
            assert!(
                requests[3].starts_with("get /api/analysis/waveform/1?type=detail&style=3band ")
            );
            assert!(requests[4].starts_with("get /api/tracks/1/phrases "));
            let detail = result.unwrap();
            assert_eq!(
                detail.waveform.entries.len(),
                1,
                "grid errors must not discard the waveform"
            );
            assert_eq!(detail.waveform.entries[0].h, 24);
            if case == "grid" {
                let beats = detail.beat_grid.visible(0.0, 2.0, 100).collect::<Vec<_>>();
                assert_eq!(beats[0].time_ms, 125.5);
                assert_eq!(beats[0].beat_in_bar, 3);
                assert_eq!(beats[2].beat_in_bar, 1);
                assert!(detail.warning.is_empty());
            } else {
                assert!(detail.beat_grid.is_empty());
                if case != "missing" {
                    assert!(detail.warning.contains("Beat grid unavailable"));
                }
            }
        }
    }

    #[test]
    fn load_uses_session_preflight_and_sends_exactly_one_command() {
        for (playing, confirm, changed) in [
            (false, false, false),
            (true, false, false),
            (true, true, false),
            (true, true, true),
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut api = Api::new(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
            let accepted = (!playing || confirm) && !changed;
            let player = serde_json::json!({"device_number":2,"track_id":7,"track_title":"Existing track","artist":"Artist","bpm":120,"key":"1A","is_playing":playing,"on_air":true,"play_state":if playing {3} else {5},"play_state_name":if playing {"PLAYING"} else {"PAUSED"}});
            let confirmed: Player = serde_json::from_value(player.clone()).unwrap();
            let mut observed = player;
            if changed {
                observed["track_id"] = serde_json::json!(8);
            }
            let mut replies = vec![
                serde_json::json!({"rev": 1, "library_session": "one"}),
                serde_json::json!([{"id":1,"title":"Test","file_path":"/music/test.flac"}]),
                serde_json::json!([]),
                serde_json::json!({"id":1,"title":"Test","file_path":"/music/test.flac"}),
                serde_json::json!({
                    "peers":[{"device_number":2,"name":"CDJ-3000","device_type":"cdj","ip":"169.254.1.2","mac":"02:00:00:00:00:02"}],
                    "players":[observed]
                }),
                serde_json::json!({"linked":true}),
            ];
            if accepted {
                replies.push(serde_json::json!({"ok":true,"track_id":1,"device_number":2}));
            }
            let task = thread::spawn(move || {
                let mut requests = vec![];
                for reply in replies {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut bytes = vec![];
                    let mut byte = [0];
                    while !bytes.ends_with(b"\r\n\r\n") {
                        stream.read_exact(&mut byte).unwrap();
                        bytes.push(byte[0]);
                    }
                    let headers = String::from_utf8(bytes).unwrap().to_lowercase();
                    let length: usize = headers
                        .lines()
                        .find_map(|l| l.strip_prefix("content-length: "))
                        .map(|s| s.parse().unwrap())
                        .unwrap_or(0);
                    let mut body = vec![0; length];
                    stream.read_exact(&mut body).unwrap();
                    requests.push((headers, body));
                    let body = reply.to_string();
                    write!(stream, "HTTP/1.1 200 OK\r\nX-Vynull-Library-Session: one\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
                requests
            });
            let library = api.library().unwrap();
            let track = &library.tracks[0];
            let peer = crate::demo::status().peers[1].clone();
            let result = api.load(track, &peer, confirm.then_some(&confirmed));
            if accepted {
                assert_eq!(result.unwrap(), (1, 2));
            } else if changed {
                assert!(result.unwrap_err().contains("track changed"));
            } else {
                assert!(result.unwrap_err().contains("confirm loading"));
            }
            let requests = task.join().unwrap();
            assert_eq!(requests.len(), if accepted { 7 } else { 6 });
            assert!(requests[0].0.starts_with("get /api/tracks/rev "));
            assert!(!requests[0].0.contains("x-vynull-library-session:"));
            assert!(
                requests[1..]
                    .iter()
                    .all(|(h, _)| h.contains("x-vynull-library-session: one"))
            );
            assert!(requests[3].0.starts_with("get /api/tracks/1 "));
            if accepted {
                assert!(requests[6].0.starts_with("post /api/load "));
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&requests[6].1).unwrap(),
                    serde_json::json!({"track_id":1,"device_number":2})
                );
            }
        }
    }
}
