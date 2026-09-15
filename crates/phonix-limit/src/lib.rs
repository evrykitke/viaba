//! In-memory fixed-window rate limiter.
//!
//! Callers supply their own keys, limits, and policies. Restarting the process
//! resets active windows.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What a decision came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Refused, with how long until the window resets.
    Refuse {
        retry_after_secs: u64,
    },
}

/// One key's counter and when it resets.
#[derive(Debug, Clone, Copy)]
struct Window {
    count: u32,
    resets_at: Instant,
}

/// Per-key request counters.
pub struct Limiter {
    windows: Mutex<HashMap<String, Window>>,
    /// Sweep expired entries after this many keys.
    sweep_above: usize,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            sweep_above: 4_096,
        }
    }

    /// Counts a request at `now` and returns the decision.
    pub fn check_at(&self, key: &str, limit: u32, window: Duration, now: Instant) -> Decision {
        // Zero disables rate limiting.
        if limit == 0 {
            return Decision::Allow;
        }

        let mut windows = match self.windows.lock() {
            Ok(guard) => guard,
            // Continue with the recovered counters after a poisoned lock.
            Err(poisoned) => poisoned.into_inner(),
        };

        if windows.len() > self.sweep_above {
            windows.retain(|_, entry| entry.resets_at > now);
        }

        let entry = windows.entry(key.to_owned()).or_insert(Window {
            count: 0,
            resets_at: now + window,
        });

        // Start a new window after expiry.
        if entry.resets_at <= now {
            *entry = Window {
                count: 0,
                resets_at: now + window,
            };
        }

        if entry.count >= limit {
            let remaining = entry.resets_at.saturating_duration_since(now);
            return Decision::Refuse {
                // A zero retry delay would cause an immediate rejected retry.
                retry_after_secs: remaining.as_secs().max(1),
            };
        }

        entry.count += 1;
        Decision::Allow
    }

    /// [`Self::check_at`] against the clock.
    pub fn check(&self, key: &str, limit: u32, window: Duration) -> Decision {
        self.check_at(key, limit, window, Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: Duration = Duration::from_secs(60);

    #[test]
    fn an_allowance_is_spent_and_then_refused() {
        let limiter = Limiter::new();
        let now = Instant::now();

        for _ in 0..3 {
            assert_eq!(
                limiter.check_at("page/1.2.3.4", 3, MINUTE, now),
                Decision::Allow
            );
        }

        assert!(matches!(
            limiter.check_at("page/1.2.3.4", 3, MINUTE, now),
            Decision::Refuse { .. }
        ));
    }

    /// Limits are tracked independently per key.
    #[test]
    fn two_keys_are_counted_apart() {
        let limiter = Limiter::new();
        let now = Instant::now();

        for _ in 0..3 {
            limiter.check_at("page/1.2.3.4", 3, MINUTE, now);
        }

        assert_eq!(
            limiter.check_at("page/5.6.7.8", 3, MINUTE, now),
            Decision::Allow
        );
    }

    #[test]
    fn the_window_resets() {
        let limiter = Limiter::new();
        let now = Instant::now();

        for _ in 0..3 {
            limiter.check_at("page/1.2.3.4", 3, MINUTE, now);
        }

        let later = now + MINUTE + Duration::from_secs(1);
        assert_eq!(
            limiter.check_at("page/1.2.3.4", 3, MINUTE, later),
            Decision::Allow
        );
    }

    /// Retry delays never round down to zero.
    #[test]
    fn a_refusal_never_says_to_come_back_immediately() {
        let limiter = Limiter::new();
        let now = Instant::now();

        limiter.check_at("page/1.2.3.4", 1, MINUTE, now);

        // Remaining whole seconds round down here.
        let nearly = now + MINUTE - Duration::from_millis(1);
        match limiter.check_at("page/1.2.3.4", 1, MINUTE, nearly) {
            Decision::Refuse { retry_after_secs } => assert_eq!(retry_after_secs, 1),
            Decision::Allow => panic!("the allowance was spent"),
        }
    }

    /// Expired windows are periodically removed.
    #[test]
    fn the_map_does_not_grow_without_bound() {
        let limiter = Limiter {
            windows: Mutex::new(HashMap::new()),
            sweep_above: 8,
        };
        let now = Instant::now();

        for n in 0..40 {
            limiter.check_at(&format!("page/10.0.0.{n}"), 3, MINUTE, now);
        }

        // Active windows are retained.
        assert!(limiter.windows.lock().expect("the lock").len() > 8);

        // The next request removes expired windows.
        let later = now + Duration::from_secs(3_600);
        limiter.check_at("page/10.0.0.99", 3, MINUTE, later);

        assert_eq!(limiter.windows.lock().expect("the lock").len(), 1);
    }

    /// Zero disables the limit.
    #[test]
    fn a_limit_of_zero_lets_everything_past() {
        let limiter = Limiter::new();
        let now = Instant::now();

        for _ in 0..100 {
            assert_eq!(
                limiter.check_at("page/1.2.3.4", 0, MINUTE, now),
                Decision::Allow
            );
        }
    }
}
