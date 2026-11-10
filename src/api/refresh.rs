// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::HashMap;

#[cfg(test)]
mod tests;

pub type Cues = HashMap<u32, Vec<Cue>>;

#[derive(Clone, Debug, Default)]
pub struct Request {
    pub generation: u64,
    pub token: u64,
    pub source: Option<u32>,
    pub detail: Option<u32>,
    pub tracks_revision: Option<u64>,
    pub cues_revision: Option<u64>,
    pub skip_cues: bool,
    pub force: bool,
}

pub struct Snapshot {
    pub tracks_revision: Option<u64>,
    pub tracks: Option<Vec<Track>>,
    pub playlists: Vec<Playlist>,
    pub playlist_tracks: Option<Vec<Track>>,
    pub cues_revision: Option<u64>,
    pub cues: Option<Cues>,
    pub skip_cues: bool,
    pub detail: Option<Detail>,
}

// Separate from both status and user commands: an expensive read must not
// delay a deck observation, load request, or cue save. The UI allows only one
// outstanding snapshot. Revisions belong to the last *applied* snapshot, so a
// discarded response can never consume a change that the user has not seen.
pub(super) fn spawn(
    origin: String,
    session: Option<String>,
    cancelled: Arc<AtomicBool>,
    requests: Receiver<Request>,
    responses: mpsc::Sender<Event>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let Ok(mut api) = Api::new(&origin) else {
            return;
        };
        api.session = session;
        api.cancelled = cancelled;
        while let Ok(request) = requests.recv() {
            if api.cancelled.load(Ordering::Relaxed) {
                break;
            }
            let result = api.refresh(&request).map(Box::new);
            if responses.send(Event::Refresh(request, result)).is_err() {
                break;
            }
            ctx.request_repaint();
        }
    });
}

impl Api {
    fn refresh(&self, request: &Request) -> Result<Snapshot, String> {
        let revision: Revision = self.get("api/tracks/rev")?;
        if self.session.is_some() && self.session != revision.library_session {
            return Err("Library changed; reconnect to reload its tracks".into());
        }
        let changed = request.force || revision.rev != request.tracks_revision;
        let tracks = if changed {
            Some(self.list("api/tracks")?)
        } else {
            None
        };
        // Playlists have no revision endpoint. Refresh their small tree and
        // only the open playlist's membership (including evaluated smart rules).
        let playlists: Vec<Playlist> = self.list("api/playlists")?;
        let playlist_tracks = match request.source {
            Some(id) if playlists.iter().any(|p| p.id == id && !p.is_folder) => {
                Some(self.list(&format!("api/playlists/{id}/tracks"))?)
            }
            _ => None,
        };
        let mut snapshot = Snapshot {
            tracks_revision: revision.rev,
            tracks,
            playlists,
            playlist_tracks,
            cues_revision: request.cues_revision,
            cues: None,
            skip_cues: request.skip_cues,
            detail: None,
        };
        if !request.skip_cues || request.force {
            match self.get::<Revision>("api/cues/rev") {
                Ok(revision) if revision.rev.is_some() => {
                    snapshot.skip_cues = false;
                    snapshot.cues_revision = revision.rev;
                    if request.force || revision.rev != request.cues_revision {
                        match self.get::<Option<Cues>>("api/cues") {
                            Ok(cues) => snapshot.cues = Some(cues.unwrap_or_default()),
                            Err(error) if unsupported(&error) => snapshot.skip_cues = true,
                            Err(error) => return Err(error),
                        }
                    }
                }
                Ok(_) => snapshot.skip_cues = true,
                Err(error) if unsupported(&error) => snapshot.skip_cues = true,
                Err(error) => return Err(error),
            }
        }
        if changed
            && let Some(id) = request.detail
            && snapshot
                .tracks
                .as_ref()
                .is_some_and(|tracks| tracks.iter().any(|t| t.id == id))
        {
            snapshot.detail = Some(self.detail(id)?);
        }
        Ok(snapshot)
    }
}

fn unsupported(error: &str) -> bool {
    ["HTTP 404", "HTTP 405", "HTTP 501"]
        .iter()
        .any(|s| error.contains(s))
}
