// SPDX-License-Identifier: GPL-3.0-or-later
use crate::model::{Cue, Peer, Player, Track, WaveEntry};
use eframe::egui::{ColorImage, Context, TextureHandle, TextureOptions};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

mod clock;
pub use clock::Clock;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Key {
    pub device: u8,
    pub track: u32,
    pub external: bool,
    pub peer: String,
    pub source: String,
    pub art: String,
    pub analysis: String,
}
impl Key {
    pub fn new(peer: Option<&Peer>, player: &Player) -> Option<Self> {
        (player.track_id > 0).then(|| Self {
            device: player.device_number,
            track: player.track_id,
            external: player.external,
            peer: peer.map_or(String::new(), |p| format!("{}|{}|{}", p.name, p.ip, p.mac)),
            source: player.source.clone(),
            art: if player.external {
                player.artwork_url.clone()
            } else {
                format!("/api/artwork/{}", player.track_id)
            },
            analysis: if player.external {
                player.analysis_url.clone()
            } else {
                format!("/api/analysis/{}", player.track_id)
            },
        })
    }
    pub fn valid_external(&self) -> bool {
        let suffix = self
            .analysis
            .strip_prefix("/api/analysis/ext/")
            .unwrap_or("");
        let fields: Vec<_> = suffix.split('/').collect();
        fields.len() == 3
            && fields[0].parse::<u8>().is_ok_and(|n| n > 0)
            && fields[1].parse::<u8>().is_ok_and(|n| n > 0)
            && fields[2].parse::<u32>().ok() == Some(self.track)
            && (self.art.is_empty() || self.art == format!("/api/artwork/ext/{suffix}"))
    }
}

#[derive(Clone, Default)]
pub struct Assets {
    pub overview: Vec<WaveEntry>,
    pub detail: Arc<[WaveEntry]>,
    pub bands: Arc<crate::waveform::Bands>,
    pub band_overview: Vec<crate::waveform::BandEntry>,
    pub sample_rate: u32,
    pub grid: Arc<crate::beat_grid::BeatGrid>,
    pub phrases: Arc<crate::phrases::Sections>,
    pub beats: Vec<f64>,
    pub cues: Vec<Cue>,
    pub duration: f64,
    pub bpm: f64,
    pub warning: String,
}
impl Assets {
    pub fn demo(track: &Track) -> Self {
        let detail = crate::demo::detail(track);
        Self {
            band_overview: crate::waveform::summarize_bands(&detail.bands.entries),
            bands: Arc::new(detail.bands),
            overview: summarize(&detail.waveform.entries),
            detail: detail.waveform.entries.into(),
            sample_rate: detail.waveform.sample_rate,
            grid: Arc::new(detail.beat_grid),
            phrases: Arc::new(detail.phrases),
            duration: f64::from(track.duration),
            bpm: track.bpm,
            beats: (0..(f64::from(track.duration) * track.bpm / 60.0).ceil() as usize)
                .map(|i| i as f64 * 60.0 / track.bpm)
                .collect(),
            cues: detail.cues,
            ..Default::default()
        }
    }
}
pub fn summarize(entries: &[WaveEntry]) -> Vec<WaveEntry> {
    entries
        .chunks(entries.len().div_ceil(1024).max(1))
        .filter_map(|c| c.iter().max_by_key(|e| e.h).copied())
        .collect()
}

/// Status gives a beat anchor and its age, not an audio clock. Allow normal
/// status latency across beat boundaries, but expire an unchanged beat anchor.
pub fn position(player: &Player, assets: &Assets, elapsed: Duration, fresh: bool) -> Option<f64> {
    if !fresh
        || player.beat_in_track == 0
        || player.beat_in_track == u32::MAX
        || !player.pitch_pct.is_finite()
    {
        return None;
    }
    let index = player.beat_in_track as usize - 1;
    let rate = (1.0 + player.pitch_pct / 100.0).clamp(0.01, 3.0);
    let start = if !assets.beats.is_empty() {
        *assets.beats.get(index)?
    } else if assets.bpm.is_finite() && assets.bpm > 0.0 {
        index as f64 * 60.0 / assets.bpm
    } else {
        return None;
    };
    let span = assets
        .beats
        .get(index + 1)
        .map(|next| next - start)
        .filter(|n| n.is_finite() && *n > 0.0)
        .unwrap_or(if assets.bpm > 0.0 {
            60.0 / assets.bpm
        } else {
            0.5
        });
    let playing = player.is_playing && player.play_state_name != "REVERSE";
    let age = f64::from(player.beat_age_ms) / 1000.0 + elapsed.as_secs_f64();
    if playing && age > (span / rate * 1.5).clamp(2.0, 4.0) {
        return None;
    }
    let phase = if playing { age * rate } else { 0.0 };
    let seconds = (start + phase.max(0.0)).max(0.0);
    Some(if assets.duration > 0.0 {
        seconds.min(assets.duration)
    } else {
        seconds
    })
}

#[derive(Clone, Default)]
pub struct View {
    pub data: Arc<Assets>,
    pub art: Option<TextureHandle>,
}
struct Entry {
    token: u64,
    view: View,
    loading: bool,
    retry: Instant,
}
struct Reply {
    key: Key,
    token: u64,
    assets: Assets,
    image: Option<ColorImage>,
}
pub struct Cache {
    entries: HashMap<Key, Entry>,
    requests: mpsc::SyncSender<(Key, u64)>,
    replies: mpsc::Receiver<Reply>,
    cancelled: Arc<AtomicBool>,
    token: u64,
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Cache {
    pub fn new(origin: String, session: Option<String>, ctx: Context) -> Self {
        let (requests, incoming) = mpsc::sync_channel::<(Key, u64)>(2);
        let incoming = Arc::new(Mutex::new(incoming));
        let (outgoing, replies) = mpsc::sync_channel(2);
        let cancelled = Arc::new(AtomicBool::new(false));
        for _ in 0..2 {
            let (incoming, outgoing, cancelled, origin, session, ctx) = (
                incoming.clone(),
                outgoing.clone(),
                cancelled.clone(),
                origin.clone(),
                session.clone(),
                ctx.clone(),
            );
            std::thread::spawn(move || {
                loop {
                    let request = incoming.lock().unwrap().recv();
                    let Ok((key, token)) = request else {
                        break;
                    };
                    if cancelled.load(Ordering::Relaxed) {
                        break;
                    }
                    let (assets, image) =
                        crate::api::deck_assets::fetch(&origin, session.clone(), &key);
                    if cancelled.load(Ordering::Relaxed)
                        || outgoing
                            .send(Reply {
                                key,
                                token,
                                assets,
                                image,
                            })
                            .is_err()
                    {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Self {
            entries: HashMap::new(),
            requests,
            replies,
            cancelled,
            token: 0,
        }
    }
    pub fn retain(&mut self, keys: &HashSet<Key>) {
        self.entries.retain(|key, _| keys.contains(key));
    }
    pub fn invalidate(&mut self, track: u32) {
        for (key, entry) in &mut self.entries {
            if !key.external && key.source.is_empty() && key.track == track {
                // Keep the current picture while fetching its replacement, but
                // reject any response that was already in flight before the edit.
                self.token += 1;
                entry.token = self.token;
                entry.loading = false;
                entry.retry = Instant::now();
            }
        }
    }
    pub fn update_cues(&mut self, track: u32, cues: &[Cue]) {
        for (key, entry) in &mut self.entries {
            if !key.external && key.source.is_empty() && key.track == track {
                Arc::make_mut(&mut entry.view.data).cues = cues.to_vec();
            }
        }
    }
    pub fn poll(&mut self, ctx: &Context) {
        while let Ok(reply) = self.replies.try_recv() {
            if let Some(entry) = self.entries.get_mut(&reply.key)
                && entry.token == reply.token
            {
                entry.loading = false;
                entry.retry = Instant::now()
                    + Duration::from_secs(if reply.assets.warning.is_empty() {
                        300
                    } else {
                        15
                    });
                entry.view = View {
                    data: Arc::new(reply.assets),
                    art: reply.image.map(|image| {
                        ctx.load_texture(
                            format!("deck-art-{:?}", reply.key),
                            image,
                            TextureOptions::LINEAR,
                        )
                    }),
                };
            }
        }
    }
    pub fn get(&mut self, key: &Key) -> View {
        let now = Instant::now();
        let entry = self.entries.entry(key.clone()).or_insert_with(|| Entry {
            token: 0,
            view: View::default(),
            loading: false,
            retry: now,
        });
        if !entry.loading && now >= entry.retry {
            self.token += 1;
            if self.requests.try_send((key.clone(), self.token)).is_ok() {
                entry.token = self.token;
                entry.loading = true;
            }
        }
        entry.view.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaced_or_disconnected_deck_assets_cannot_receive_old_replies() {
        let (requests, incoming) = mpsc::sync_channel(4);
        let (outgoing, replies) = mpsc::sync_channel(4);
        let mut cache = Cache {
            entries: HashMap::new(),
            requests,
            replies,
            cancelled: Arc::new(AtomicBool::new(false)),
            token: 0,
        };
        let key = Key::new(
            None,
            &Player {
                track_id: 1,
                device_number: 1,
                ..Default::default()
            },
        )
        .unwrap();
        cache.get(&key);
        let (_, old) = incoming.recv().unwrap();
        cache.invalidate(1);
        cache.get(&key);
        let (_, new) = incoming.recv().unwrap();
        outgoing
            .send(Reply {
                key: key.clone(),
                token: old,
                assets: Assets {
                    duration: 999.0,
                    ..Default::default()
                },
                image: None,
            })
            .unwrap();
        cache.poll(&Context::default());
        assert_eq!(cache.get(&key).data.duration, 0.0);
        outgoing
            .send(Reply {
                key: key.clone(),
                token: new,
                assets: Assets {
                    duration: 120.0,
                    ..Default::default()
                },
                image: None,
            })
            .unwrap();
        cache.poll(&Context::default());
        assert_eq!(cache.get(&key).data.duration, 120.0);
        let external = Key {
            external: true,
            ..key.clone()
        };
        let external_view = cache.get(&key);
        cache.entries.insert(
            external.clone(),
            Entry {
                token: 0,
                view: external_view,
                loading: false,
                retry: Instant::now() + Duration::from_secs(60),
            },
        );
        cache.update_cues(
            1,
            &[Cue {
                number: 1,
                time_ms: 5000,
                ..Default::default()
            }],
        );
        cache.invalidate(1);
        assert_eq!(
            cache.get(&key).data.duration,
            120.0,
            "keep waveform while refreshing"
        );
        assert_eq!(cache.get(&key).data.cues[0].time_ms, 5000);
        assert!(
            cache.get(&external).data.cues.is_empty(),
            "external IDs must not alias library IDs"
        );
        cache.retain(&HashSet::new());
        assert!(cache.entries.is_empty());
    }
    #[test]
    fn deck_clock_uses_saved_beats_pitch_and_expires_stalled_anchors() {
        let assets = Assets {
            beats: vec![0.125, 0.625, 1.125],
            duration: 3.0,
            bpm: 120.0,
            ..Default::default()
        };
        let mut p = Player {
            beat_in_track: 2,
            beat_age_ms: 100,
            is_playing: true,
            pitch_pct: 10.0,
            ..Default::default()
        };
        assert!(
            (position(&p, &assets, Duration::from_millis(100), true).unwrap() - 0.845).abs() < 1e-6
        );
        assert_eq!(position(&p, &assets, Duration::from_secs(10), true), None);
        assert_eq!(position(&p, &assets, Duration::ZERO, false), None);
        p.is_playing = false;
        assert_eq!(
            position(&p, &assets, Duration::from_secs(10), true),
            Some(0.625)
        );
        p.beat_in_track = 0;
        assert_eq!(position(&p, &assets, Duration::ZERO, true), None);
        p.beat_in_track = 4;
        assert_eq!(position(&p, &assets, Duration::ZERO, true), None);
    }
    #[test]
    fn external_source_identity_never_aliases_library_ids_or_another_usb() {
        let mut p = Player {
            device_number: 1,
            track_id: 7,
            external: true,
            analysis_url: "/api/analysis/ext/2/3/7".into(),
            artwork_url: "/api/artwork/ext/2/3/7".into(),
            ..Default::default()
        };
        let first = Key::new(None, &p).unwrap();
        assert!(first.valid_external());
        p.external = false;
        assert_ne!(first, Key::new(None, &p).unwrap());
        p.external = true;
        p.analysis_url = "/api/analysis/ext/3/3/7".into();
        assert!(!Key::new(None, &p).unwrap().valid_external());
        p.analysis_url = "https://other/api/analysis/ext/2/3/7".into();
        assert!(!Key::new(None, &p).unwrap().valid_external());
    }
}
