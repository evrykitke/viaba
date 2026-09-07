//! Fixed windows, in memory: how often one anonymous caller may ask.
//!
//! This is the counting only. Which requests are counted, what a caller is
//! keyed on, and how many they get are policy, and the two applications that
//! use this disagree about all three - `phonix-server` sorts requests into
//! four tiers by path and keys an API call on its credential, while the public
//! site has one tier and one key. Neither of those belongs in here.
//!
//! What did belong in one place is the arithmetic. It was written once for the
//! server and copying it for the site would have meant a limiter that quietly
//! stopped agreeing with itself - which for this kind of code is not a
//! cosmetic problem, because the copy that is subtly wrong is the one nobody
//! looks at.
//!
//! # A counter and an expiry per key
//!
//! Not a sliding window and not a token bucket. Both are better shaped, and
//! neither is worth the arithmetic against an attacker whose actual budget is
//! "one workspace an hour instead of thousands".
//!
//! In memory rather than Redis, which is a choice with a consequence: a
//! process restart resets every window. That is a real hole, and a smaller one
//! than a limiter that disappears whenever Redis hiccups.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// What a decision came to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    /// Refused, with how long until the window resets.
    Refuse { retry_after_secs: u64 },
}

/// One key's counter and when it resets.
#[derive(Debug, Clone, Copy)]
struct Window {
    count: u32,
    resets_at: Instant,
}

/// The counters.
///
/// A single `Mutex<HashMap>`, held for the few instructions it takes to bump an
/// integer. Sharding it would matter under contention a limiter this coarse
/// will not see, and an uncontended mutex costs a couple of nanoseconds.
pub struct Limiter {
    windows: Mutex<HashMap<String, Window>>,
    /// Number of entries past which a sweep runs on the next decision.
    ///
    /// Without this the map grows for as long as the process lives, one entry
    /// per address that ever arrived - a slow memory leak with an
    /// attacker-controlled rate.
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

    /// Count one request against `key` and say whether it may proceed.
    ///
    /// `key` is the whole key. A caller that counts more than one kind of
    /// request puts the kind in the string, because two allowances sharing a
    /// key would spend each other.
    ///
    /// `now` is a parameter so tests can move time without sleeping.
    pub fn check_at(&self, key: &str, limit: u32, window: Duration, now: Instant) -> Decision {
        // A limit of zero would refuse everybody for ever, including whoever is
        // trying to reach the screen that fixes it. Read as "not limited",
        // which is what somebody typing 0 into a config file meant.
        if limit == 0 {
            return Decision::Allow;
        }

        let mut windows = match self.windows.lock() {
            Ok(guard) => guard,
            // A panic while another thread held the lock. The counts are just
            // integers - nothing is half-written - so the sane answer is to
            // keep limiting rather than to fail open or to panic in turn.
            Err(poisoned) => poisoned.into_inner(),
        };

        if windows.len() > self.sweep_above {
            windows.retain(|_, entry| entry.resets_at > now);
        }

        let entry = windows.entry(key.to_owned()).or_insert(Window {
            count: 0,
            resets_at: now + window,
        });

        // Expired: this is the first request of a new window, not the next of
        // an old one.
        if entry.resets_at <= now {
            *entry = Window {
                count: 0,
                resets_at: now + window,
            };
        }

        if entry.count >= limit {
            let remaining = entry.resets_at.saturating_duration_since(now);
            return Decision::Refuse {
                // Never zero: `Retry-After: 0` invites an immediate retry that
                // is certain to be refused again.
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

    /// The property the whole design rests on: one caller running out must not
    /// spend anybody else's allowance.
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

    /// `Retry-After: 0` invites a retry certain to be refused again.
    #[test]
    fn a_refusal_never_says_to_come_back_immediately() {
        let limiter = Limiter::new();
        let now = Instant::now();

        limiter.check_at("page/1.2.3.4", 1, MINUTE, now);

        // A hair before the window closes, so the remaining whole seconds
        // round to nothing.
        let nearly = now + MINUTE - Duration::from_millis(1);
        match limiter.check_at("page/1.2.3.4", 1, MINUTE, nearly) {
            Decision::Refuse { retry_after_secs } => assert_eq!(retry_after_secs, 1),
            Decision::Allow => panic!("the allowance was spent"),
        }
    }

    /// One entry per address that ever arrived, kept for the life of the
    /// process, is a slow memory leak at a rate the caller chooses.
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

        // Everything is still live, so nothing is swept yet.
        assert!(limiter.windows.lock().expect("the lock").len() > 8);

        // An hour later every window has expired, and the next decision clears
        // them out rather than keeping one entry per address for ever.
        let later = now + Duration::from_secs(3_600);
        limiter.check_at("page/10.0.0.99", 3, MINUTE, later);

        assert_eq!(limiter.windows.lock().expect("the lock").len(), 1);
    }

    /// Zero means "not limited", not "refuse everybody for ever".
    #[test]
    fn a_limit_of_zero_lets_everything_past() {
        let limiter = Limiter::new();
        let now = Instant::now();

        for _ in 0..100 {
            assert_eq!(limiter.check_at("page/1.2.3.4", 0, MINUTE, now), Decision::Allow);
        }
    }
}
