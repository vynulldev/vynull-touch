// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::HashMap;

const INTERVAL: Duration = Duration::from_secs(5);

struct Entry {
    count: Option<usize>,
    checked: Instant,
    error: bool,
}

struct Pending {
    id: u32,
    started: Instant,
    receive: Receiver<Result<usize, String>>,
}

/// Smart playlist membership is evaluated by Vynull, not included in the tree.
/// Fetch only visible rows, one at a time, independently of playback and commands.
#[derive(Default)]
pub struct Counts {
    entries: HashMap<u32, Entry>,
    pending: Option<Pending>,
}

impl Counts {
    pub fn poll(&mut self, playlists: &[Playlist]) {
        self.entries
            .retain(|id, _| playlists.iter().any(|p| p.id == *id && p.is_smart));
        let Some(pending) = &self.pending else {
            return;
        };
        let result = match pending.receive.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => Err("Count worker stopped".into()),
        };
        let id = pending.id;
        // Opening a playlist may have supplied a newer total in the meantime.
        if playlists.iter().any(|p| p.id == id && p.is_smart)
            && self
                .entries
                .get(&id)
                .is_none_or(|entry| entry.checked <= pending.started)
        {
            let count = result.as_ref().ok().copied().or_else(|| self.count(id));
            self.entries.insert(
                id,
                Entry {
                    count,
                    checked: Instant::now(),
                    error: result.is_err(),
                },
            );
        }
        self.pending = None;
    }

    pub fn request(&mut self, id: u32, origin: &str, session: Option<&str>, ctx: &egui::Context) {
        ctx.request_repaint_after(Duration::from_secs(1));
        if self.pending.is_some()
            || self
                .entries
                .get(&id)
                .is_some_and(|entry| entry.checked.elapsed() < INTERVAL)
        {
            return;
        }
        let (send, receive) = mpsc::sync_channel(1);
        self.pending = Some(Pending {
            id,
            started: Instant::now(),
            receive,
        });
        let origin = origin.to_owned();
        let session = session.map(str::to_owned);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let result = Api::new(&origin).and_then(|mut api| {
                api.session = session;
                // Ignore metadata while counting, preserving the normal session,
                // response-size and redirect protections of library requests.
                api.json::<Option<Vec<serde::de::IgnoredAny>>>(
                    Method::GET,
                    &format!("api/playlists/{id}/tracks"),
                    None,
                    5,
                )
                .map(|tracks| tracks.unwrap_or_default().len())
            });
            let _ = send.send(result);
            ctx.request_repaint();
        });
    }

    pub fn remember(&mut self, id: u32, count: usize) {
        self.entries.insert(
            id,
            Entry {
                count: Some(count),
                checked: Instant::now(),
                error: false,
            },
        );
    }

    pub fn count(&self, id: u32) -> Option<usize> {
        self.entries.get(&id).and_then(|entry| entry.count)
    }

    pub fn failed(&self, id: u32) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_failures_preserve_totals_and_older_reads_cannot_replace_new_membership() {
        let playlists = vec![Playlist {
            id: 9,
            is_smart: true,
            ..Default::default()
        }];
        let mut counts = Counts::default();
        counts.remember(9, 42);
        let (send, receive) = mpsc::channel();
        counts.pending = Some(Pending {
            id: 9,
            started: Instant::now(),
            receive,
        });
        send.send(Err("Offline".into())).unwrap();
        counts.poll(&playlists);
        assert_eq!(counts.count(9), Some(42));
        assert!(counts.failed(9));
        assert!(counts.pending.is_none());

        let (send, receive) = mpsc::channel();
        counts.pending = Some(Pending {
            id: 9,
            started: Instant::now() - Duration::from_secs(1),
            receive,
        });
        counts.remember(9, 1);
        send.send(Ok(50)).unwrap();
        counts.poll(&playlists);
        assert_eq!(counts.count(9), Some(1));
        assert!(!counts.failed(9));
        counts.poll(&[]);
        assert_eq!(counts.count(9), None, "deleted playlists leave the cache");
    }
}
