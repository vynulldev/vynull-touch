// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

const LIMIT: u64 = 64 * 1024;
const INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, Deserialize)]
pub struct Info {
    pub version: String,
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub dirty: bool,
}

impl Info {
    pub fn description(&self) -> String {
        let mut text = self.version.clone();
        if !self.commit.is_empty() {
            text.push_str(&format!(
                " · {}",
                self.commit.chars().take(12).collect::<String>()
            ));
        }
        if self.dirty {
            text.push_str(" · modified");
        }
        text
    }
}

#[derive(Default)]
pub struct Version {
    pub info: Option<Info>,
    pub updated: Option<Instant>,
    pub error: String,
    pub failures: u64,
    pending: Option<Receiver<Result<Option<Info>, String>>>,
    next: Option<Instant>,
}

impl Version {
    pub fn update(&mut self, ctx: &Context, origin: &str, demo: bool, active: bool) {
        if let Some(result) = self.pending.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(result) => Some(result),
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(Err("Server version worker stopped.".into()))
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }) {
            self.pending = None;
            self.next = Some(Instant::now() + INTERVAL);
            match result {
                Ok(info) => {
                    self.info = info;
                    self.updated = Some(Instant::now());
                    self.error.clear();
                }
                Err(error) => {
                    self.error = error;
                    self.failures = self.failures.saturating_add(1);
                }
            }
        }
        if !active || demo {
            return;
        }
        if self.pending.is_none() && self.next.is_none_or(|at| Instant::now() >= at) {
            let (send, receive) = mpsc::sync_channel(1);
            let origin = origin.to_owned();
            let ctx = ctx.clone();
            // Version requests never share the player-status or command worker.
            std::thread::spawn(move || {
                let result = Client::builder()
                    .connect_timeout(Duration::from_secs(2))
                    .timeout(Duration::from_secs(3))
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
                    .map_err(|e| e.to_string())
                    .and_then(|client| fetch(&client, &origin));
                let _ = send.send(result);
                ctx.request_repaint();
            });
            self.pending = Some(receive);
        }
        ctx.request_repaint_after(Duration::from_secs(1));
    }

    pub fn description(&self, demo: bool) -> String {
        if demo {
            "Demo · no connected daemon".into()
        } else if let Some(info) = &self.info {
            info.description()
        } else if self.updated.is_some() {
            "Not reported by this server".into()
        } else if !self.error.is_empty() {
            "Unavailable".into()
        } else {
            "Checking…".into()
        }
    }
}

fn fetch(client: &Client, origin: &str) -> Result<Option<Info>, String> {
    let url = crate::api::validate_origin(origin)?
        .join("api/version")
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .header("Cache-Control", "no-cache")
        .send()
        .map_err(|_| "Cannot reach the server version endpoint.".to_owned())?;
    if matches!(response.status().as_u16(), 404 | 405 | 501) {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(format!(
            "Server version: HTTP {}",
            response.status().as_u16()
        ));
    }
    if response.content_length().is_some_and(|n| n > LIMIT) {
        return Err("Server version response exceeds 64 KiB.".into());
    }
    let mut bytes = Vec::new();
    response
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read the server version.".to_owned())?;
    if bytes.len() as u64 > LIMIT {
        return Err("Server version response exceeds 64 KiB.".into());
    }
    let info: Info = serde_json::from_slice(&bytes)
        .map_err(|_| "Invalid server version response.".to_owned())?;
    if info.version.trim().is_empty()
        || [&info.version, &info.commit, &info.date]
            .iter()
            .any(|v| v.len() > 512 || v.chars().any(char::is_control))
    {
        return Err("Invalid server version fields.".into());
    }
    Ok(Some(info))
}

#[cfg(test)]
mod tests;
