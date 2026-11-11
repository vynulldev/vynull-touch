// SPDX-License-Identifier: GPL-3.0-or-later
use super::{Assets, Player};
use std::time::Instant;

#[derive(Default)]
pub struct Clock {
    anchor: Option<Anchor>,
    last_position: Option<f64>,
    cue_beat: Option<(u32, Instant)>,
}

struct Anchor {
    sampled: Instant,
    at: Instant,
    position: f64,
    rate: f64,
    correction: f64,
    settle: f64,
    beat: u32,
    age_ms: u32,
    transport: (bool, u8, bool),
}

impl Anchor {
    fn project(&self, now: Instant) -> f64 {
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        self.position
            + elapsed * self.rate
            + self.correction * (elapsed / self.settle).clamp(0.0, 1.0)
    }
}

impl Clock {
    /// A display anchor only, never evidence of a current player position.
    pub fn last_position(&self) -> Option<f64> {
        self.last_position
    }

    pub fn position(
        &mut self,
        player: &Player,
        assets: &Assets,
        sampled: Instant,
        now: Instant,
        fresh: bool,
    ) -> Option<f64> {
        // Older servers send age=0 throughout CUE PLAY when the deck's playing
        // flag is clear. Keep the first observation of this beat across polls;
        // treating every zero as a new beat would repeatedly rewind the clock.
        // Retain it even after an expired anchor, until the beat/state changes.
        let cue_play = player.play_state == 7 && player.is_forward_playback();
        let beat_sampled = if cue_play && fresh {
            let anchor = self.cue_beat.get_or_insert((player.beat_in_track, sampled));
            if anchor.0 != player.beat_in_track {
                *anchor = (player.beat_in_track, sampled);
            }
            anchor.1
        } else {
            self.cue_beat = None;
            sampled
        };
        let phase_sampled = if cue_play && !player.is_playing && player.beat_age_ms == 0 {
            beat_sampled
        } else {
            sampled
        };
        let Some(observed) = super::position(
            player,
            assets,
            now.saturating_duration_since(phase_sampled),
            fresh,
        ) else {
            self.anchor = None;
            return None;
        };
        let reverse = player.play_state_name == "REVERSE";
        let transport = (player.is_forward_playback(), player.play_state, reverse);
        let rate = if player.is_forward_playback() {
            (1.0 + player.pitch_pct / 100.0).clamp(0.01, 3.0)
        } else {
            0.0
        };
        if self.anchor.as_ref().is_none_or(|a| sampled > a.sampled) {
            let mut start = observed;
            let mut correction = 0.0;
            if let Some(old) = &self.anchor {
                let predicted = old.project(now);
                let error = observed - predicted;
                let reset_phase = player.beat_in_track == old.beat
                    && player.beat_age_ms.saturating_add(30) < old.age_ms;
                // Cue/loop/seek and transport changes take effect immediately.
                // Small timing errors ease in without freezing or reversing.
                if rate > 0.0
                    && transport == old.transport
                    && player.beat_in_track >= old.beat
                    && !reset_phase
                    && error.abs() <= 0.3
                {
                    start = predicted;
                    correction = error;
                }
            }
            self.anchor = Some(Anchor {
                sampled,
                at: now,
                position: start,
                rate,
                correction,
                settle: (correction.abs() / (rate.max(0.01) * 0.25)).max(0.25),
                beat: player.beat_in_track,
                age_ms: player.beat_age_ms,
                transport,
            });
        }
        let position = self.anchor.as_ref()?.project(now).max(0.0);
        let position = if assets.duration > 0.0 {
            position.min(assets.duration)
        } else {
            position
        };
        self.last_position = Some(position);
        Some(position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn assets() -> Assets {
        Assets {
            beats: (0..500).map(|i| i as f64 * 0.5).collect(),
            bpm: 120.0,
            duration: 250.0,
            ..Default::default()
        }
    }
    fn player(seconds: f64) -> Player {
        Player {
            beat_in_track: (seconds / 0.5).floor() as u32 + 1,
            beat_age_ms: ((seconds % 0.5) * 1000.0).round() as u32,
            is_playing: true,
            play_state: 3,
            play_state_name: "PLAYING".into(),
            ..Default::default()
        }
    }
    fn at(start: Instant, seconds: f64) -> Instant {
        start + Duration::from_secs_f64(seconds)
    }

    fn cue_play(seconds: f64) -> Player {
        Player {
            is_playing: false,
            play_state: 7,
            play_state_name: "CUE PLAY".into(),
            beat_age_ms: 0, // Older API suppresses phase when the playing flag is clear.
            ..player(seconds)
        }
    }

    #[test]
    fn held_cue_advances_smoothly_across_repeated_zero_age_status_and_delayed_beats() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let mut p = cue_play(10.0);
        let mut sampled = start;
        let mut previous = 10.0;
        for frame in 0..360 {
            let t = frame as f64 / 60.0;
            // Quarter-second polls, with the first beat change delivered 150 ms late.
            if frame >= 9 && (frame - 9) % 15 == 0 {
                p = cue_play(10.0 + t - 0.15);
                sampled = at(start, t);
            }
            let position = clock
                .position(&p, &assets, sampled, at(start, t), true)
                .unwrap();
            if frame > 0 {
                assert!(
                    (0.01..0.025).contains(&(position - previous)),
                    "cue-play jumped or froze at frame {frame}: {position} after {previous}"
                );
            }
            assert!((position - (10.0 + t)).abs() < 0.2);
            previous = position;
        }
        // Releasing CUE returns immediately, even within the current beat.
        let now = at(start, 6.0);
        let cued = Player {
            play_state: 6,
            play_state_name: "CUED".into(),
            ..cue_play(15.5)
        };
        assert_eq!(clock.position(&cued, &assets, now, now, true), Some(15.5));
        assert_eq!(
            clock.position(&cued, &assets, now, at(start, 6.2), true),
            Some(15.5)
        );
        let now = at(start, 6.3);
        assert_eq!(
            clock.position(&cue_play(15.5), &assets, now, now, true),
            Some(15.5)
        );
        assert_eq!(
            clock.position(&cue_play(15.5), &assets, now, at(start, 6.4), true),
            Some(15.6)
        );
    }

    #[test]
    fn cue_play_phase_fallback_expires_without_restarting_on_repeated_status() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let p = cue_play(10.0);
        assert_eq!(clock.position(&p, &assets, start, start, true), Some(10.0));
        for i in 1..20 {
            let now = at(start, i as f64 * 0.25);
            let result = clock.position(&p, &assets, now, now, true);
            if i > 8 {
                assert!(result.is_none(), "stalled cue beat must stay expired");
            }
        }
        let now = at(start, 5.0);
        assert_eq!(
            clock.position(&cue_play(12.0), &assets, now, now, true),
            Some(12.0)
        );
        let now = at(start, 5.1);
        assert!(clock.position(&p, &assets, now, now, false).is_none());
    }

    #[test]
    fn cue_play_uses_reported_phase_when_available_and_applies_pitch() {
        let start = Instant::now();
        let assets = assets();
        for playing_flag in [false, true] {
            let mut clock = Clock::default();
            let p = Player {
                is_playing: playing_flag,
                beat_age_ms: 100,
                pitch_pct: 10.0,
                ..cue_play(10.0)
            };
            let before = clock.position(&p, &assets, start, start, true).unwrap();
            let after = clock
                .position(&p, &assets, start, at(start, 0.1), true)
                .unwrap();
            assert!((before - 10.11).abs() < 1e-6);
            assert!((after - before - 0.11).abs() < 1e-6);
        }
    }

    #[test]
    fn missing_paused_timing_retains_only_a_static_display_anchor() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let p = player(12.25);
        assert_eq!(clock.position(&p, &assets, start, start, true), Some(12.25));
        for (state, beat) in [(5, 0), (6, u32::MAX)] {
            let stopped = Player {
                is_playing: false,
                play_state: state,
                beat_in_track: beat,
                ..p.clone()
            };
            assert!(
                clock
                    .position(&stopped, &assets, at(start, 1.0), at(start, 1.0), true)
                    .is_none()
            );
            assert!(
                clock
                    .position(&stopped, &assets, at(start, 30.0), at(start, 30.0), true)
                    .is_none()
            );
            assert_eq!(clock.last_position(), Some(12.25));
        }
        let now = at(start, 31.0);
        assert_eq!(
            clock.position(&player(32.0), &assets, now, now, true),
            Some(32.0)
        );
        assert_eq!(Clock::default().last_position(), None);
    }

    #[test]
    fn delayed_quarter_second_status_does_not_pause_at_each_beat() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let mut p = player(0.0);
        let mut sampled = start;
        let mut previous = 0.0;
        for frame in 0..360 {
            let t = frame as f64 / 60.0;
            // Poll every 250 ms with a 150 ms delivery delay. Each old beat
            // remains visible past its end, reproducing the previous stutter.
            if frame >= 9 && (frame - 9) % 15 == 0 {
                let sample_time = t - 0.15;
                p = player(sample_time);
                sampled = at(start, sample_time);
            }
            let value = clock
                .position(&p, &assets, sampled, at(start, t), true)
                .unwrap();
            assert!((value - t).abs() < 0.002, "frame {frame}: {value} vs {t}");
            if frame > 0 {
                assert!(value - previous > 0.015, "clock paused on frame {frame}");
            }
            previous = value;
        }
    }

    #[test]
    fn small_phase_corrections_are_smooth_but_loops_seeks_and_pause_are_immediate() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let mut p = player(0.0);
        let mut sampled = start;
        let mut previous = 0.0;
        for frame in 0..180 {
            let t = frame as f64 / 60.0;
            if frame % 15 == 0 {
                let jitter = if frame % 30 == 0 { 0.04 } else { 0.0 };
                p = player(t + jitter);
                sampled = at(start, t);
            }
            let value = clock
                .position(&p, &assets, sampled, at(start, t), true)
                .unwrap();
            if frame > 0 {
                assert!(
                    (0.01..0.025).contains(&(value - previous)),
                    "jump: {}",
                    value - previous
                );
            }
            previous = value;
        }
        let now = at(start, 3.0);
        let mut jumped = player(20.1);
        assert!((clock.position(&jumped, &assets, now, now, true).unwrap() - 20.1).abs() < 0.001);
        jumped = player(4.05);
        let now = at(start, 3.1);
        assert!((clock.position(&jumped, &assets, now, now, true).unwrap() - 4.05).abs() < 0.001);
        jumped.is_playing = false;
        jumped.play_state = 5;
        let now = at(start, 3.2);
        assert_eq!(clock.position(&jumped, &assets, now, now, true), Some(4.0));
        assert_eq!(
            clock.position(&jumped, &assets, now, at(start, 3.4), true),
            Some(4.0)
        );
        // A short loop can reset the phase without changing the beat counter.
        let now = at(start, 3.5);
        let resumed = player(4.1);
        clock.position(&resumed, &assets, now, now, true);
        let now = at(start, 3.6);
        assert!(
            (clock
                .position(&player(4.02), &assets, now, now, true)
                .unwrap()
                - 4.02)
                .abs()
                < 0.001
        );
    }

    #[test]
    fn stalled_beats_expire_even_when_http_status_keeps_arriving_and_pitch_changes_flow() {
        let start = Instant::now();
        let assets = assets();
        let mut clock = Clock::default();
        let p = player(1.0);
        assert_eq!(clock.position(&p, &assets, start, start, true), Some(1.0));
        let mut faster = player(1.25);
        faster.pitch_pct = 10.0;
        let now = at(start, 0.25);
        let before = clock.position(&faster, &assets, now, now, true).unwrap();
        let after = clock
            .position(&faster, &assets, now, at(start, 0.35), true)
            .unwrap();
        assert!((0.10..0.14).contains(&(after - before)));
        let stalled = Player {
            beat_age_ms: 2100,
            ..p
        };
        let now = at(start, 2.1);
        assert!(clock.position(&stalled, &assets, now, now, true).is_none());
        assert!(clock.position(&faster, &assets, now, now, false).is_none());
        assert_eq!(
            clock.position(&player(8.0), &assets, now, now, true),
            Some(8.0)
        );
    }
}
