// SPDX-License-Identifier: GPL-3.0-or-later
mod engine;
mod http;
mod output;
mod wav;
pub use output::{OutputDevice, devices};

use eframe::egui::Context;
use engine::Engine;
use std::time::Duration;
use tokio::sync::watch;
use wav::Pcm;

#[derive(Clone, Debug)]
pub struct Request {
    pub epoch: u64,
    pub revision: u64,
    pub seek: u64,
    pub track: Option<(u32, u32)>,
    pub origin: String,
    pub session: Option<String>,
    pub demo: bool,
    pub playing: bool,
    pub dragging: bool,
    pub activated: bool,
    pub seconds: f64,
    pub loop_region: Option<crate::loops::Region>,
    pub volume: f32,
    pub device: Option<OutputDevice>,
}
impl Default for Request {
    fn default() -> Self {
        Self {
            epoch: 0,
            revision: 0,
            seek: 0,
            track: None,
            origin: String::new(),
            session: None,
            demo: false,
            playing: false,
            dragging: false,
            activated: false,
            seconds: 0.0,
            loop_region: None,
            volume: 0.5,
            device: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Phase {
    #[default]
    Empty,
    Loading,
    Paused,
    Playing,
    Buffering,
    Ended,
    Error,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub epoch: u64,
    pub revision: u64,
    pub phase: Phase,
    pub position: f64,
    pub duration: f64,
    pub output_name: Option<String>,
    pub error: Option<String>,
}
impl Snapshot {
    pub fn accepts(&self, request: &Request) -> bool {
        self.epoch == request.epoch && self.revision == request.revision
    }
}

pub struct Controller {
    pub request: Request,
    pub state: Snapshot,
    sender: watch::Sender<Request>,
    results: watch::Receiver<Snapshot>,
}
impl Controller {
    pub fn new(ctx: Context) -> Self {
        let (sender, requests) = watch::channel(Request::default());
        let (results, snapshots) = watch::channel(Snapshot::default());
        std::thread::Builder::new()
            .name("vynull-preview".into())
            .spawn(move || {
                match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime.block_on(run(requests, results, ctx)),
                    Err(error) => publish(
                        &results,
                        &ctx,
                        Snapshot {
                            phase: Phase::Error,
                            error: Some(error.to_string()),
                            ..Default::default()
                        },
                    ),
                }
            })
            .expect("start preview worker");
        Self {
            request: Request::default(),
            state: Snapshot::default(),
            sender,
            results: snapshots,
        }
    }
    pub fn submit(&mut self) {
        self.request.revision += 1;
        if self.sender.send(self.request.clone()).is_err() {
            self.state = Snapshot {
                epoch: self.request.epoch,
                revision: self.request.revision,
                phase: Phase::Error,
                error: Some("Audio worker stopped; reopen the UI".into()),
                ..Default::default()
            };
        }
    }
    pub fn load(&mut self, track: (u32, u32), origin: String, session: Option<String>, demo: bool) {
        self.request.epoch += 1;
        self.request.track = Some(track);
        self.request.origin = origin;
        self.request.session = session;
        self.request.demo = demo;
        self.request.playing = false;
        self.request.dragging = false;
        self.request.activated = false;
        self.request.seconds = 0.0;
        self.request.loop_region = None;
        self.request.seek += 1;
        self.state = Snapshot::default();
        self.submit();
    }
    pub fn stop(&mut self) {
        if self.request.track.is_none() {
            return;
        }
        self.request.epoch += 1;
        self.request.track = None;
        self.request.loop_region = None;
        self.request.playing = false;
        self.request.dragging = false;
        self.request.activated = false;
        self.state = Snapshot::default();
        self.submit();
    }
    pub fn seek(&mut self, seconds: f64, dragging: bool) {
        if self
            .request
            .loop_region
            .is_some_and(|r| !r.contains(seconds))
        {
            self.request.loop_region = None;
        }
        self.request.seconds = seconds.max(0.0);
        self.request.dragging = dragging;
        self.request.seek += 1;
        self.submit();
    }
    pub fn start_loop(&mut self, region: crate::loops::Region) {
        self.request.loop_region = Some(region);
        self.seek(region.start, false);
    }
    pub fn change_loop(&mut self, region: Option<crate::loops::Region>, position: f64) {
        self.request.loop_region = region;
        self.request.seconds = region.map_or(position, |r| r.position(position));
        self.submit();
    }
    pub fn toggle(&mut self, position: f64) {
        if self.request.playing
            && self.state.phase != Phase::Error
            && self.state.phase != Phase::Ended
        {
            self.request.playing = false;
            if self.state.phase == Phase::Loading {
                self.request.activated = false;
            }
        } else {
            self.request.playing = true;
            self.request.activated = true;
            self.request.seconds = if self.state.phase == Phase::Ended {
                self.request.loop_region.map_or(0.0, |r| r.start)
            } else {
                self.request
                    .loop_region
                    .map_or(position, |r| r.position(position))
            };
            self.request.seek += 1;
        }
        self.submit();
    }
    pub fn pause(&mut self) {
        if self.request.playing {
            self.request.playing = false;
            if self.state.phase == Phase::Loading {
                self.request.activated = false;
            }
            self.submit();
        }
    }
    pub fn poll(&mut self) -> bool {
        if self.results.has_changed().unwrap_or(false) {
            let state = self.results.borrow_and_update().clone();
            if state.accepts(&self.request) {
                self.state = state;
                return true;
            }
        }
        false
    }
    #[cfg(test)]
    pub(crate) fn test() -> (Self, watch::Receiver<Request>, watch::Sender<Snapshot>) {
        let (sender, requests) = watch::channel(Request::default());
        let (results, snapshots) = watch::channel(Snapshot::default());
        (
            Self {
                request: Request::default(),
                state: Snapshot::default(),
                sender,
                results: snapshots,
            },
            requests,
            results,
        )
    }
}

enum Media {
    Live(http::AudioFile),
    Demo(Pcm),
}
impl Media {
    async fn prepare(request: &Request) -> Result<Self, String> {
        let (id, duration) = request.track.ok_or("No track selected")?;
        if request.demo {
            Ok(Self::Demo(Pcm {
                channels: 2,
                sample_rate: 48_000,
                data_offset: 0,
                frames: u64::from(duration) * 48_000,
            }))
        } else {
            http::AudioFile::open(&request.origin, id, request.session.as_deref())
                .await
                .map(Self::Live)
        }
    }
    fn pcm(&self) -> Pcm {
        match self {
            Self::Live(f) => f.pcm,
            Self::Demo(p) => *p,
        }
    }
    async fn samples(&self, frame: u64, count: u64) -> Result<Vec<f32>, String> {
        match self {
            Self::Live(file) => file.samples(frame, count).await,
            Self::Demo(pcm) => {
                let mut samples = Vec::with_capacity(count as usize * usize::from(pcm.channels));
                for frame in frame..frame + count {
                    let t = frame as f64 / f64::from(pcm.sample_rate);
                    let sample = (0.08 * (t * 220.0 * std::f64::consts::TAU).sin()) as f32;
                    samples.extend(std::iter::repeat_n(sample, usize::from(pcm.channels)));
                }
                Ok(samples)
            }
        }
    }
}

fn publish(results: &watch::Sender<Snapshot>, ctx: &Context, state: Snapshot) {
    if *results.borrow() != state {
        results.send_replace(state);
        ctx.request_repaint();
    }
}
fn state(request: &Request, engine: &Engine, pcm: Pcm) -> Snapshot {
    let phase = if engine.position() >= pcm.duration() || engine.ended() {
        Phase::Ended
    } else if !request.playing || request.dragging {
        Phase::Paused
    } else if engine.buffering() {
        Phase::Buffering
    } else {
        Phase::Playing
    };
    Snapshot {
        epoch: request.epoch,
        revision: request.revision,
        phase,
        position: engine.position(),
        duration: pcm.duration(),
        output_name: engine.output_name(),
        error: None,
    }
}

async fn run(
    mut requests: watch::Receiver<Request>,
    results: watch::Sender<Snapshot>,
    ctx: Context,
) {
    let mut engine = Engine::default();
    let mut epoch = u64::MAX;
    let mut media: Option<Media> = None;
    let mut next_frame = 0;
    let mut seek = u64::MAX;
    let mut device = None;
    let mut failure: Option<(u64, String)> = None;
    'requests: loop {
        let request = requests.borrow_and_update().clone();
        if epoch != request.epoch {
            epoch = request.epoch;
            engine.reset(None, 0);
            media = None;
            seek = u64::MAX;
            failure = None;
        }
        if device != request.device {
            device = request.device.clone();
            engine.set_output(device.clone());
            next_frame = engine.frame();
            failure = None;
        }
        if !request.playing || request.dragging {
            engine.pause();
        }
        if request.track.is_none() || !request.activated {
            publish(
                &results,
                &ctx,
                Snapshot {
                    epoch,
                    revision: request.revision,
                    phase: if request.track.is_none() {
                        Phase::Empty
                    } else {
                        Phase::Paused
                    },
                    position: request.seconds,
                    ..Default::default()
                },
            );
            if requests.changed().await.is_err() {
                return;
            }
            continue;
        }
        if let Some((revision, error)) = &failure {
            if *revision == request.revision {
                publish(
                    &results,
                    &ctx,
                    Snapshot {
                        epoch,
                        revision: *revision,
                        phase: Phase::Error,
                        position: engine.position(),
                        duration: media.as_ref().map_or(0.0, |m| m.pcm().duration()),
                        error: Some(error.clone()),
                        output_name: engine.output_name(),
                    },
                );
                if requests.changed().await.is_err() {
                    return;
                }
                continue;
            }
            failure = None;
        }
        if media.is_none() {
            publish(
                &results,
                &ctx,
                Snapshot {
                    epoch,
                    revision: request.revision,
                    phase: Phase::Loading,
                    position: request.seconds,
                    ..Default::default()
                },
            );
            tokio::select! {
                changed = requests.changed() => { if changed.is_err() { return; } continue; }
                result = Media::prepare(&request) => match result {
                    Ok(prepared) => media = Some(prepared),
                    Err(e) => { failure = Some((request.revision, e)); continue; }
                }
            }
        }
        let media = media.as_ref().unwrap();
        let pcm = media.pcm();
        let loop_range = match request
            .loop_region
            .map(|r| crate::loops::Frames::new(r, pcm.sample_rate, pcm.frames))
            .transpose()
        {
            Ok(range) => range,
            Err(error) => {
                engine.pause();
                failure = Some((request.revision, error));
                continue;
            }
        };
        if seek != request.seek || engine.loop_range != loop_range {
            next_frame = if seek != request.seek {
                pcm.frame_at(request.seconds)
            } else {
                engine.frame()
            };
            engine.reset_loop(Some(pcm), next_frame, loop_range);
            next_frame = engine.frame();
            seek = request.seek;
        }
        if engine.failed() {
            next_frame = engine.frame();
            engine.reset_loop(Some(pcm), next_frame, loop_range);
            failure = Some((
                request.revision,
                "Audio output disconnected. Press Play to retry.".into(),
            ));
            continue;
        }
        if request.playing
            && !request.dragging
            && engine.position() < pcm.duration()
            && !engine.ended()
        {
            if let Err(e) = engine.play(request.volume) {
                engine.pause();
                next_frame = engine.frame();
                engine.reset_loop(Some(pcm), next_frame, loop_range);
                failure = Some((request.revision, e));
                continue;
            }
        } else {
            engine.pause();
        }
        engine.volume(request.volume);
        if let Some(range) = loop_range {
            next_frame = range.constrain(next_frame);
        }
        if loop_range.is_none() && next_frame == pcm.frames {
            engine.finish();
        }
        publish(&results, &ctx, state(&request, &engine, pcm));
        let end = loop_range.map_or(pcm.frames, |r| r.end);
        let count = (end - next_frame).min(u64::from(pcm.sample_rate) / 4);
        if !request.dragging && count > 0 && engine.available() >= count as usize {
            let samples = media.samples(next_frame, count);
            tokio::pin!(samples);
            loop {
                tokio::select! {
                    changed = requests.changed() => { if changed.is_err() { return; } continue 'requests; }
                    result = &mut samples => {
                        match result {
                            Ok(samples) => { engine.push(&samples); next_frame += count; }
                            Err(e) => { engine.pause(); failure = Some((request.revision, e)); }
                        }
                        continue 'requests;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(33)) => {
                        if engine.failed() { continue 'requests; }
                        publish(&results, &ctx, state(&request, &engine, pcm));
                    }
                }
            }
        }
        tokio::select! {
            changed = requests.changed() => if changed.is_err() { return; },
            _ = tokio::time::sleep(Duration::from_millis(33)) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Explicitly opt in: streams a real track at low volume through the chosen
    // machine's default output. Normal cargo test never opens audio hardware.
    #[tokio::test]
    #[ignore = "requires VYNULL_AUDIO_TEST_ORIGIN, VYNULL_AUDIO_TEST_TRACK and a working audio output"]
    async fn live_preview_play_pause_seek_and_end() {
        let origin = std::env::var("VYNULL_AUDIO_TEST_ORIGIN").unwrap();
        let id = std::env::var("VYNULL_AUDIO_TEST_TRACK")
            .unwrap()
            .parse::<u32>()
            .unwrap();
        let revision: serde_json::Value = reqwest::get(format!("{origin}/api/tracks/rev"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let session = revision["library_session"].as_str().unwrap().to_owned();
        let file = http::AudioFile::open(&origin, id, Some(&session))
            .await
            .unwrap();
        assert!(file.pcm.duration() > 21.0);
        let samples = file
            .samples(file.pcm.frame_at(8.0), u64::from(file.pcm.sample_rate) / 4)
            .await
            .unwrap();
        assert!(
            samples.iter().any(|s| s.abs() > 0.001),
            "fixture should contain audible samples"
        );
        let mut request = Request {
            epoch: 1,
            revision: 1,
            track: Some((id, file.pcm.duration() as u32)),
            origin,
            session: Some(session),
            activated: true,
            playing: true,
            seconds: 8.0,
            volume: 0.02,
            ..Default::default()
        };
        let (sender, requests) = watch::channel(request.clone());
        let (results, mut snapshots) = watch::channel(Snapshot::default());
        let worker = tokio::spawn(run(requests, results, Context::default()));
        async fn wait(
            snapshots: &mut watch::Receiver<Snapshot>,
            revision: u64,
            predicate: impl Fn(&Snapshot) -> bool,
        ) -> Snapshot {
            tokio::time::timeout(Duration::from_secs(20), async {
                loop {
                    let s = snapshots.borrow_and_update().clone();
                    assert_ne!(s.phase, Phase::Error, "{:?}", s.error);
                    if s.revision == revision && predicate(&s) {
                        return s;
                    }
                    snapshots.changed().await.unwrap();
                }
            })
            .await
            .expect("audio state timed out")
        }
        let playing = wait(&mut snapshots, 1, |s| {
            s.position > 8.2 && s.phase == Phase::Playing
        })
        .await;
        println!(
            "Played decoded PCM via {:?} at {:.3}s",
            playing.output_name, playing.position
        );
        request.revision = 2;
        request.playing = false;
        sender.send(request.clone()).unwrap();
        let paused = wait(&mut snapshots, 2, |s| s.phase == Phase::Paused).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(
            (snapshots.borrow().position - paused.position).abs() < 0.03,
            "paused clock advanced"
        );
        request.revision = 3;
        request.seek += 1;
        request.seconds = 20.0;
        sender.send(request.clone()).unwrap();
        let seek = wait(&mut snapshots, 3, |s| s.phase == Phase::Paused).await;
        assert!((seek.position - 20.0).abs() < 0.001);
        request.revision = 4;
        request.playing = true;
        sender.send(request.clone()).unwrap();
        wait(&mut snapshots, 4, |s| {
            s.position > 20.2 && s.phase == Phase::Playing
        })
        .await;
        request.revision = 5;
        request.loop_region = Some(crate::loops::Region {
            start: 8.0,
            end: 8.5,
        });
        request.seek += 1;
        request.seconds = 8.0;
        sender.send(request.clone()).unwrap();
        let first = wait(&mut snapshots, 5, |s| {
            s.phase == Phase::Playing && s.position > 8.05
        })
        .await;
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut previous = first.position;
            let mut wraps = 0;
            while wraps < 3 {
                snapshots.changed().await.unwrap();
                let state = snapshots.borrow_and_update().clone();
                assert_eq!(
                    state.phase,
                    Phase::Playing,
                    "loop should remain buffered: {state:?}"
                );
                assert!((8.0..8.5).contains(&state.position));
                if state.position < previous - 0.1 {
                    wraps += 1;
                }
                previous = state.position;
            }
        })
        .await
        .expect("loop did not wrap three times");
        request.revision = 6;
        request.loop_region = Some(crate::loops::Region {
            start: 8.0,
            end: 9.0,
        });
        sender.send(request.clone()).unwrap();
        wait(&mut snapshots, 6, |s| {
            s.phase == Phase::Playing && s.position > 8.6
        })
        .await;
        request.revision = 7;
        request.playing = false;
        sender.send(request.clone()).unwrap();
        let paused = wait(&mut snapshots, 7, |s| s.phase == Phase::Paused).await;
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!((snapshots.borrow().position - paused.position).abs() < 0.03);
        request.revision = 8;
        request.playing = true;
        sender.send(request.clone()).unwrap();
        wait(&mut snapshots, 8, |s| s.phase == Phase::Playing).await;
        request.revision = 9;
        request.loop_region = None;
        request.seconds = 0.0; // Exit must use the consumed-frame clock, not this stale seek target.
        sender.send(request.clone()).unwrap();
        wait(&mut snapshots, 9, |s| {
            s.phase == Phase::Playing && s.position > 9.1
        })
        .await;
        request.revision = 10;
        request.seek += 1;
        request.seconds = file.pcm.duration() - 0.2;
        sender.send(request.clone()).unwrap();
        let ended = wait(&mut snapshots, 10, |s| s.phase == Phase::Ended).await;
        assert!((ended.position - file.pcm.duration()).abs() < 0.001);
        sender
            .send(Request {
                epoch: 2,
                revision: 11,
                ..Default::default()
            })
            .unwrap();
        wait(&mut snapshots, 11, |s| s.phase == Phase::Empty).await;
        drop(sender);
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap();
        println!(
            "Verified pause, precise seek, three loop wraps, resize, loop pause/resume/exit, EOF and stop through the native audio callback"
        );
    }
    #[test]
    fn transport_preserves_pause_during_seeks_and_rejects_old_track_and_seek_replies() {
        let (mut c, _requests, results) = Controller::test();
        c.load(
            (1, 120),
            "http://localhost".into(),
            Some("one".into()),
            true,
        );
        c.seek(32.0, true);
        assert!(!c.request.playing);
        c.seek(33.0, false);
        c.toggle(33.0);
        assert!(c.request.playing);
        c.seek(40.0, true);
        assert!(c.request.playing && c.request.dragging);
        c.seek(41.0, false);
        let old = Snapshot {
            epoch: c.request.epoch,
            revision: c.request.revision,
            position: 41.0,
            ..Default::default()
        };
        c.seek(90.0, false);
        results.send(old.clone()).unwrap();
        assert!(!c.poll());
        c.load(
            (2, 120),
            "http://localhost".into(),
            Some("one".into()),
            true,
        );
        results.send(old).unwrap();
        assert!(!c.poll());
        assert!(!c.request.playing);
        c.stop();
        assert!(c.request.track.is_none());
    }
    #[tokio::test]
    async fn closing_or_switching_away_cancels_audio_preparation() {
        use tokio::{io::AsyncReadExt, net::TcpListener};
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let request = Request {
            track: Some((1, 120)),
            origin: format!("http://{}", listener.local_addr().unwrap()),
            session: Some("one".into()),
            activated: true,
            playing: true,
            ..Default::default()
        };
        let (sender, requests) = watch::channel(request.clone());
        let (results, mut snapshots) = watch::channel(Snapshot::default());
        let worker = tokio::spawn(run(requests, results, Context::default()));
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(3), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut byte = [0];
        socket.read_exact(&mut byte).await.unwrap();
        sender
            .send(Request {
                epoch: 1,
                revision: 1,
                ..Default::default()
            })
            .unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if snapshots.borrow_and_update().epoch == 1 {
                    break;
                }
                snapshots.changed().await.unwrap();
            }
        })
        .await
        .unwrap();
        assert_eq!(snapshots.borrow().phase, Phase::Empty);
        drop(sender);
        tokio::time::timeout(Duration::from_secs(2), worker)
            .await
            .unwrap()
            .unwrap();
    }
}
