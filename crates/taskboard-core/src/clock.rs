//! Wall clock and hybrid logical clock (HLC).
//!
//! HLC strings sort in causal order on this device even if the wall clock
//! steps backwards, which is what phase-2 sync needs for last-writer-wins.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current UTC time in milliseconds.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hlc {
    ms: i64,
    counter: u32,
}

impl Hlc {
    /// Advances the clock for a local event at wall time `now` and returns its stamp.
    pub fn tick(&mut self, now: i64) -> String {
        if now > self.ms {
            self.ms = now;
            self.counter = 0;
        } else {
            self.counter = self.counter.saturating_add(1);
        }
        self.encode()
    }

    pub fn ms(&self) -> i64 {
        self.ms
    }

    /// Fixed-width hex so lexical order equals (ms, counter) order.
    pub fn encode(&self) -> String {
        format!("{:012x}-{:06x}", self.ms.max(0), self.counter)
    }

    pub fn decode(s: &str) -> Option<Self> {
        let (ms, counter) = s.split_once('-')?;
        Some(Self {
            ms: i64::from_str_radix(ms, 16).ok()?,
            counter: u32::from_str_radix(counter, 16).ok()?,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_even_when_clock_goes_back() {
        let mut hlc = Hlc::default();
        let a = hlc.tick(1_000);
        let b = hlc.tick(900);
        let c = hlc.tick(900);
        let d = hlc.tick(2_000);
        assert!(a < b && b < c && c < d);
    }

    #[test]
    fn encode_decode_round_trip() {
        let mut hlc = Hlc::default();
        let s = hlc.tick(1_760_000_000_000);
        assert_eq!(Hlc::decode(&s).unwrap(), hlc);
    }
}
