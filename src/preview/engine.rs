// SPDX-License-Identifier: GPL-3.0-or-later
// The same bounded, consumed-frame clock used by vynull-desktop's preview.
use super::{
    output::{self, Output, OutputDevice},
    wav::Pcm,
};
use rodio::{Player, Source};
use rtrb::{Consumer, Producer, RingBuffer};
use std::{
    num::NonZero,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

#[derive(Default)]
struct Clock {
    frames: AtomicU64,
    buffering: AtomicBool,
    complete: AtomicBool,
}

struct PcmSource {
    consumer: Consumer<f32>,
    clock: Arc<Clock>,
    finished: Arc<AtomicBool>,
    channels: NonZero<u16>,
    rate: NonZero<u32>,
    channel: u16,
    silent: bool,
}

impl Iterator for PcmSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        if self.channel == 0 {
            self.silent = self.consumer.slots() < usize::from(self.channels.get());
            if self.silent && self.finished.load(Ordering::Acquire) {
                self.silent = self.consumer.slots() < usize::from(self.channels.get());
                if self.silent {
                    self.clock.complete.store(true, Ordering::Relaxed);
                    return None;
                }
            }
            self.clock.buffering.store(self.silent, Ordering::Relaxed);
        }
        let value = if self.silent {
            0.0
        } else {
            self.consumer.pop().unwrap_or(0.0)
        };
        self.channel += 1;
        if self.channel == self.channels.get() {
            self.channel = 0;
            if !self.silent {
                self.clock.frames.fetch_add(1, Ordering::Relaxed);
            }
        }
        Some(value)
    }
}

impl Source for PcmSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> NonZero<u16> {
        self.channels
    }
    fn sample_rate(&self) -> NonZero<u32> {
        self.rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[derive(Default)]
pub struct Engine {
    output: Option<Output>,
    device: Option<OutputDevice>,
    player: Option<Player>,
    source: Option<PcmSource>,
    producer: Option<Producer<f32>>,
    clock: Arc<Clock>,
    finished: Arc<AtomicBool>,
    pcm: Option<Pcm>,
    start: u64,
    pub loop_range: Option<crate::loops::Frames>,
}

impl Engine {
    pub fn reset(&mut self, pcm: Option<Pcm>, start: u64) {
        self.reset_loop(pcm, start, None);
    }
    pub fn reset_loop(
        &mut self,
        pcm: Option<Pcm>,
        start: u64,
        range: Option<crate::loops::Frames>,
    ) {
        if let Some(player) = self.player.take() {
            player.stop();
        }
        if self.failed() || pcm.is_none() {
            self.output = None;
        }
        self.source = None;
        self.producer = None;
        self.clock = Arc::default();
        self.finished = Arc::default();
        self.pcm = pcm;
        self.loop_range = range;
        self.start = pcm.map_or(0, |p| {
            range.map_or(start.min(p.frames), |r| r.constrain(start))
        });
        if let Some(pcm) = pcm {
            let (producer, consumer) =
                RingBuffer::new(pcm.sample_rate as usize * usize::from(pcm.channels));
            self.producer = Some(producer);
            self.source = Some(PcmSource {
                consumer,
                clock: self.clock.clone(),
                finished: self.finished.clone(),
                channels: NonZero::new(pcm.channels).unwrap(),
                rate: NonZero::new(pcm.sample_rate).unwrap(),
                channel: 0,
                silent: false,
            });
        }
    }
    pub fn set_output(&mut self, device: Option<OutputDevice>) {
        if self.device != device {
            let position = self.frame();
            self.reset_loop(self.pcm, position, self.loop_range);
            self.output = None;
            self.device = device;
        }
    }
    pub fn play(&mut self, volume: f32) -> Result<(), String> {
        if self.failed() {
            return Err("Audio output disconnected. Press Play to retry.".into());
        }
        if self.output.is_none() {
            self.output = Some(output::open(self.device.as_ref())?);
        }
        if self.player.is_none() {
            let (player, source) = Player::new();
            player.pause();
            player.set_volume(volume.clamp(0.0, 1.0));
            if let Some(source) = self.source.take() {
                player.append(source);
            }
            self.output.as_ref().unwrap().sink.mixer().add(source);
            self.player = Some(player);
        }
        self.volume(volume);
        self.player.as_ref().unwrap().play();
        Ok(())
    }
    pub fn pause(&self) {
        if let Some(p) = &self.player {
            p.pause();
        }
    }
    pub fn volume(&self, volume: f32) {
        if let Some(p) = &self.player {
            p.set_volume(volume.clamp(0.0, 1.0));
        }
    }
    pub fn available(&self) -> usize {
        self.pcm
            .zip(self.producer.as_ref())
            .map_or(0, |(pcm, p)| p.slots() / usize::from(pcm.channels))
    }
    pub fn push(&mut self, samples: &[f32]) {
        if let Some(p) = &mut self.producer {
            assert!(p.slots() >= samples.len());
            for &sample in samples {
                let _ = p.push(sample);
            }
        }
    }
    pub fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }
    pub fn frame(&self) -> u64 {
        if let Some(range) = self.loop_range {
            return range.position(self.start, self.clock.frames.load(Ordering::Relaxed));
        }
        self.start
            .saturating_add(self.clock.frames.load(Ordering::Relaxed))
            .min(self.pcm.map_or(0, |p| p.frames))
    }
    pub fn position(&self) -> f64 {
        self.pcm
            .map_or(0.0, |p| self.frame() as f64 / f64::from(p.sample_rate))
    }
    pub fn buffering(&self) -> bool {
        self.clock.buffering.load(Ordering::Relaxed)
    }
    pub fn ended(&self) -> bool {
        self.clock.complete.load(Ordering::Relaxed)
    }
    pub fn failed(&self) -> bool {
        self.output
            .as_ref()
            .is_some_and(|o| o.fault.load(Ordering::Relaxed))
    }
    pub fn output_name(&self) -> Option<String> {
        self.output.as_ref().map(|o| o.name.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loop_clock_wraps_on_complete_frames_and_exit_discards_queued_repeats() {
        let mut engine = Engine::default();
        let range = crate::loops::Frames {
            start: 48000,
            end: 48003,
        };
        engine.reset_loop(Some(pcm()), 48001, Some(range));
        let expected: Vec<_> = (0..20).map(|n| range.position(48001, n)).collect();
        let samples: Vec<_> = expected
            .iter()
            .flat_map(|f| [(*f - 48000) as f32 + 0.1, -0.5])
            .collect();
        engine.push(&samples);
        for (n, &frame) in expected.iter().enumerate() {
            assert_eq!(engine.frame(), frame);
            assert_eq!(engine.source.as_mut().unwrap().next(), Some(samples[n * 2]));
            assert_eq!(
                engine.frame(),
                frame,
                "half a stereo frame must not advance the clock"
            );
            assert_eq!(engine.source.as_mut().unwrap().next(), Some(-0.5));
            assert!(!engine.buffering());
        }
        let paused_at = engine.frame();
        for _ in 0..4 {
            assert_eq!(engine.source.as_mut().unwrap().next(), Some(0.0));
        }
        assert_eq!(
            engine.frame(),
            paused_at,
            "underrun must not move the loop clock"
        );
        assert!(!engine.ended());
        engine.push(&[0.9, 0.9]);
        engine.reset_loop(Some(pcm()), paused_at, None);
        engine.push(&[0.2, 0.3]);
        assert_eq!(engine.source.as_mut().unwrap().next(), Some(0.2));
        assert_eq!(engine.source.as_mut().unwrap().next(), Some(0.3));
        assert_eq!(engine.frame(), paused_at + 1);
    }
    fn pcm() -> Pcm {
        Pcm {
            channels: 2,
            sample_rate: 48_000,
            data_offset: 44,
            frames: 96_000,
        }
    }
    #[test]
    fn clock_advances_only_for_complete_played_frames_and_freezes_on_underrun() {
        let mut engine = Engine::default();
        engine.reset(Some(pcm()), 48_000);
        engine.push(&[0.25, -0.5]);
        let mut source = engine.source.take().unwrap();
        assert_eq!(source.next(), Some(0.25));
        assert_eq!(engine.frame(), 48_000);
        assert_eq!(source.next(), Some(-0.5));
        assert_eq!(engine.frame(), 48_001);
        for _ in 0..20 {
            assert_eq!(source.next(), Some(0.0));
        }
        assert_eq!(engine.frame(), 48_001);
        assert!(engine.buffering());
        engine.push(&[0.1, 0.2]);
        engine.finish();
        assert_eq!(source.next(), Some(0.1));
        assert_eq!(source.next(), Some(0.2));
        assert_eq!(source.next(), None);
        assert!(engine.ended());
    }
    #[test]
    fn seeking_and_output_changes_discard_old_samples_without_growing_the_queue() {
        let mut engine = Engine::default();
        engine.reset(Some(pcm()), 48_000);
        assert_eq!(engine.available(), 48_000);
        engine.push(&[0.8, 0.8]);
        engine.set_output(Some(OutputDevice {
            id: "test:one".into(),
            name: "Test".into(),
            host: "Test".into(),
        }));
        assert_eq!(engine.position(), 1.0);
        assert_eq!(engine.available(), 48_000);
        engine.reset(Some(pcm()), 24_000);
        engine.push(&[0.1, 0.2]);
        assert_eq!(engine.source.as_mut().unwrap().next(), Some(0.1));
        assert_eq!(engine.position(), 0.5);
    }
}
