// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Default)]
pub enum Connection {
    #[default]
    Connected,
    Connecting {
        attempt: u32,
    },
    Waiting {
        attempt: u32,
        until: Instant,
    },
}

pub(super) fn backoff(attempt: u32) -> Duration {
    Duration::from_secs((1_u64 << attempt.saturating_sub(1).min(5)).min(30))
}

// Only the initial library read and status polling use this wait. Commands,
// including mutations, are processed once and never enter a retry queue.
pub(super) fn wait(cancelled: &AtomicBool, retry: &AtomicBool, until: Instant) -> bool {
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return false;
        }
        if retry.swap(false, Ordering::Relaxed) || Instant::now() >= until {
            return true;
        }
        thread::sleep(
            until
                .saturating_duration_since(Instant::now())
                .min(Duration::from_millis(50)),
        );
    }
}
