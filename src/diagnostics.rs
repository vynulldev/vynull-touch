// SPDX-License-Identifier: GPL-3.0-or-later
use eframe::egui::Context;
use reqwest::{StatusCode, blocking::Client};
use serde::Deserialize;
use std::{
    io::Read,
    sync::mpsc::{self, Receiver, SyncSender},
    time::{Duration, Instant},
};

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 500;
const INTERVAL: Duration = Duration::from_secs(2);

pub mod version;

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Entry {
    pub seq: u64,
    pub time: i64,
    pub line: String,
}

impl Entry {
    pub fn timestamp(&self) -> String {
        let seconds = self.time.div_euclid(1000).rem_euclid(86_400);
        format!(
            "{:02}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    }
}

struct Worker {
    requests: SyncSender<u64>,
    replies: Receiver<(u64, Result<Vec<Entry>, String>)>,
}

impl Worker {
    fn spawn(origin: &str, ctx: Context) -> Self {
        let (requests, incoming) = mpsc::sync_channel(1);
        let (outgoing, replies) = mpsc::sync_channel(1);
        let origin = origin.to_owned();
        std::thread::spawn(move || {
            let client = Client::builder()
                .connect_timeout(Duration::from_secs(2))
                .timeout(Duration::from_secs(3))
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!(
                    env!("CARGO_PKG_NAME"),
                    "/",
                    env!("CARGO_PKG_VERSION")
                ))
                .build()
                .map_err(|e| e.to_string());
            while let Ok(epoch) = incoming.recv() {
                let result = match &client {
                    Ok(client) => fetch(client, &origin),
                    Err(error) => Err(error.clone()),
                };
                if outgoing.send((epoch, result)).is_err() {
                    break;
                }
                ctx.request_repaint();
            }
        });
        Self { requests, replies }
    }
}

fn fetch(client: &Client, origin: &str) -> Result<Vec<Entry>, String> {
    let url = crate::api::validate_origin(origin)?
        .join("api/diag/logs?since=0")
        .map_err(|e| e.to_string())?;
    // Fetch the server's bounded window so a restarted daemon (whose sequence
    // starts again at one) cannot leave the log viewer stuck on an old cursor.
    let response = client
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .map_err(|_| "Cannot reach server logs. Retrying while live.".to_owned())?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("This Vynull server does not provide the log endpoint.".into());
    }
    if !response.status().is_success() {
        return Err(format!("Server logs: HTTP {}", response.status().as_u16()));
    }
    if response.content_length().is_some_and(|n| n > MAX_BYTES) {
        return Err("Server log response exceeds 2 MiB.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read server logs. Retrying while live.".to_owned())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Server log response exceeds 2 MiB.".into());
    }
    let mut entries = serde_json::from_slice::<Option<Vec<Entry>>>(&bytes)
        .map_err(|_| "Invalid server log response.".to_owned())?
        .unwrap_or_default();
    if entries.iter().any(|e| e.seq == 0)
        || entries.windows(2).any(|pair| pair[0].seq >= pair[1].seq)
    {
        return Err("Invalid server log sequence.".into());
    }
    if entries.len() > MAX_ENTRIES {
        entries.drain(..entries.len() - MAX_ENTRIES);
    }
    Ok(entries)
}

#[derive(Default)]
pub struct Logs {
    pub entries: Vec<Entry>,
    pub paused: bool,
    pub error: String,
    pub updated: Option<Instant>,
    pub revision: u64,
    pub failures: u64,
    worker: Option<Worker>,
    pending: bool,
    epoch: u64,
    next_poll: Option<Instant>,
}

impl Logs {
    pub fn pause(&mut self) {
        if !self.paused {
            self.paused = true;
            self.epoch += 1;
        }
    }

    pub fn resume(&mut self) {
        self.paused = false;
        self.next_poll = None;
    }

    pub fn update(&mut self, ctx: &Context, origin: &str, demo: bool, active: bool) {
        if let Some(worker) = &self.worker {
            while let Ok((epoch, result)) = worker.replies.try_recv() {
                self.pending = false;
                if epoch != self.epoch || self.paused || !active {
                    continue;
                }
                self.next_poll = Some(Instant::now() + INTERVAL);
                match result {
                    Ok(entries) => {
                        if self.entries != entries {
                            self.entries = entries;
                            self.revision += 1;
                        }
                        self.error.clear();
                        self.updated = Some(Instant::now());
                    }
                    Err(error) => {
                        self.failures = self.failures.saturating_add(1);
                        self.error = error;
                    }
                }
            }
        }
        if !active {
            self.next_poll = None;
            return;
        }
        if self.paused {
            return;
        }
        if demo {
            if self.updated.is_none() {
                self.entries = demo_entries();
                self.revision += 1;
                self.updated = Some(Instant::now());
            }
            return;
        }
        if !self.pending && self.next_poll.is_none_or(|at| Instant::now() >= at) {
            let worker = self
                .worker
                .get_or_insert_with(|| Worker::spawn(origin, ctx.clone()));
            match worker.requests.try_send(self.epoch) {
                Ok(()) => self.pending = true,
                Err(mpsc::TrySendError::Full(_)) => (),
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    self.error = "Server log worker stopped. Reconnect in Setup.".into();
                    self.next_poll = Some(Instant::now() + INTERVAL);
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    pub fn copy_text(&self) -> String {
        self.entries
            .iter()
            .map(|e| format!("{} UTC  {}", e.timestamp(), e.line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn demo_entries() -> Vec<Entry> {
    [
        "vynull: starting demo server",
        "library: loaded 96 tracks and 4 playlists",
        "analysis: cached waveforms and beat grids ready",
        "api: listening on http://127.0.0.1:9443",
        "link: advertising Vynull library",
        "discovery: player 1 connected (CDJ-3000)",
        "discovery: player 2 connected (CDJ-3000)",
        "dbserver: player 1 requested library tracks",
        "nfs: serving After the Rain to player 1",
        "status: player 1 PLAYING · 120.00 BPM · MASTER",
        "analysis: preparing artwork and high-resolution spectral waveform for the selected track; existing player status updates continue while analysis completes",
        "WARN artwork: no embedded cover; using placeholder",
        "analysis: waveform ready (150 samples per second)",
        "dbserver: player 2 requested playlist Warm-up selections",
        "status: player 2 CUED · 120.00 BPM",
        "link: 2 players connected",
    ].into_iter().enumerate().map(|(i, line)| Entry {
        seq: i as u64 + 1, time: 1_794_052_800_000 + i as i64 * 1000, line: line.into(),
    }).collect()
}

#[cfg(test)]
mod tests;
