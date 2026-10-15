//! In-memory rate limits with a bounded map (one instance; a shared store
//! comes with a second instance).
//!
//! GCRA: each key holds one instant, its "theoretical arrival time". A key
//! allows `burst` requests at once, then one per `interval`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub type Key = [u8; 16];

#[derive(Debug)]
pub struct RateLimiter {
    interval: Duration,
    tolerance: Duration,
    capacity: usize,
    state: Mutex<HashMap<Key, Instant>>,
}

impl RateLimiter {
    /// `burst` requests at once, then one per `interval`, for at most
    /// `capacity` keys.
    pub fn new(burst: u32, interval: Duration, capacity: usize) -> Self {
        assert!(burst >= 1 && capacity >= 1);
        Self {
            interval,
            tolerance: interval * (burst - 1),
            capacity,
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Records a request for `key`; returns whether it is allowed.
    pub fn check(&self, key: &Key) -> bool {
        self.check_at(key, Instant::now())
    }

    pub fn check_at(&self, key: &Key, now: Instant) -> bool {
        let mut map = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let tat = map.get(key).copied().unwrap_or(now).max(now);
        if tat > now + self.tolerance {
            return false;
        }
        if !map.contains_key(key) && map.len() >= self.capacity {
            // Drop keys whose allowance is full again, then, if still full,
            // an arbitrary tenth: under a flood from many addresses we would
            // rather forget some than refuse everyone.
            map.retain(|_, t| *t > now);
            if map.len() >= self.capacity {
                let drop: Vec<Key> = map.keys().take(self.capacity / 10 + 1).copied().collect();
                for k in drop {
                    map.remove(&k);
                }
            }
        }
        map.insert(*key, tat + self.interval);
        true
    }

    pub fn len(&self) -> usize {
        self.state.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The waitlist's limits.
#[derive(Debug)]
pub struct Limits {
    /// Per IP (IPv4 address or IPv6 /64), hashed.
    pub ip: RateLimiter,
    /// Per email address, hashed.
    pub email: RateLimiter,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            // 8 at once, then one a minute.
            ip: RateLimiter::new(8, Duration::from_secs(60), 50_000),
            // 3 at once, then one every 20 minutes.
            email: RateLimiter::new(3, Duration::from_secs(20 * 60), 50_000),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_then_steady_rate() {
        let rl = RateLimiter::new(3, Duration::from_secs(10), 100);
        let k = [1u8; 16];
        let t0 = Instant::now();
        assert!(rl.check_at(&k, t0));
        assert!(rl.check_at(&k, t0));
        assert!(rl.check_at(&k, t0));
        assert!(!rl.check_at(&k, t0), "burst used up");
        assert!(!rl.check_at(&k, t0 + Duration::from_secs(5)));
        assert!(
            rl.check_at(&k, t0 + Duration::from_secs(10)),
            "one more after an interval"
        );
        assert!(rl.check_at(&[2u8; 16], t0), "other keys are independent");
    }

    #[test]
    fn map_stays_bounded() {
        let rl = RateLimiter::new(1, Duration::from_secs(60), 100);
        let now = Instant::now();
        for i in 0..1000u32 {
            let mut k = [0u8; 16];
            k[..4].copy_from_slice(&i.to_le_bytes());
            assert!(rl.check_at(&k, now));
        }
        assert!(rl.len() <= 100);
    }
}
