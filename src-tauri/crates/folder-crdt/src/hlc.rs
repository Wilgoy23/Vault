//! Hybrid logical clocks: the clock every write is stamped with.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::field::DeviceId;

/// The device id given to data migrated from a format that had no device
/// ids. Never handed out to a real device.
pub const LEGACY_DEVICE: DeviceId = 0;

/// A timestamp: wall-clock milliseconds, a counter for writes within one
/// millisecond (or after the wall clock went backwards), and the device that
/// made it. Ordered in that order. The device makes every timestamp unique,
/// so two writes never tie.
///
/// Serialized as `[wall_ms, counter, device]`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Zeroize)]
#[serde(from = "(u64, u32, DeviceId)", into = "(u64, u32, DeviceId)")]
pub struct Hlc {
    pub wall_ms: u64,
    pub counter: u32,
    pub device: DeviceId,
}

impl Hlc {
    pub const fn new(wall_ms: u64, counter: u32, device: DeviceId) -> Self {
        Hlc { wall_ms, counter, device }
    }

    /// A timestamp for data migrated from a format that stored unix seconds.
    /// `counter` orders writes that share a second.
    pub const fn legacy(secs: u64, counter: u32) -> Self {
        Hlc::new(secs.saturating_mul(1000), counter, LEGACY_DEVICE)
    }

    /// The wall-clock part in unix seconds, for showing to people.
    pub const fn secs(&self) -> u64 {
        self.wall_ms / 1000
    }
}

impl From<(u64, u32, DeviceId)> for Hlc {
    fn from((wall_ms, counter, device): (u64, u32, DeviceId)) -> Self {
        Hlc { wall_ms, counter, device }
    }
}

impl From<Hlc> for (u64, u32, DeviceId) {
    fn from(h: Hlc) -> Self {
        (h.wall_ms, h.counter, h.device)
    }
}

/// One device's clock. Every timestamp it returns is greater than every
/// one it returned or [observed](HlcClock::observe) before, so an edit always
/// beats the data it was made on, even when this device's wall clock is
/// behind another's.
#[derive(Debug, Clone)]
pub struct HlcClock {
    device: DeviceId,
    /// Wall time and counter of the newest timestamp issued or observed
    last: (u64, u32),
}

impl HlcClock {
    pub fn new(device: DeviceId) -> Self {
        HlcClock { device, last: (0, 0) }
    }

    pub fn device(&self) -> DeviceId {
        self.device
    }

    /// A new timestamp at the current system time.
    pub fn now(&mut self) -> Hlc {
        let wall_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or_default();
        self.tick_at(wall_ms)
    }

    /// A new timestamp with the wall clock reading `wall_ms`.
    pub fn tick_at(&mut self, wall_ms: u64) -> Hlc {
        let (last_ms, last_counter) = self.last;
        self.last = if wall_ms > last_ms {
            (wall_ms, 0)
        } else if last_counter == u32::MAX {
            (last_ms + 1, 0)
        } else {
            (last_ms, last_counter + 1)
        };
        Hlc::new(self.last.0, self.last.1, self.device)
    }

    /// Notes a timestamp seen in data from another device, so the next one
    /// issued here sorts after it.
    pub fn observe(&mut self, seen: Hlc) {
        self.last = self.last.max((seen.wall_ms, seen.counter));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_always_increase_even_when_the_wall_clock_goes_back() {
        let mut c = HlcClock::new(7);
        let a = c.tick_at(1000);
        let b = c.tick_at(1000);
        let d = c.tick_at(500);
        let e = c.tick_at(2000);
        assert!(a < b && b < d && d < e);
        assert_eq!(d, Hlc::new(1000, 2, 7));
        assert_eq!(e, Hlc::new(2000, 0, 7));
    }

    #[test]
    fn a_tick_after_observing_another_device_sorts_after_it() {
        let mut slow = HlcClock::new(1);
        let theirs = Hlc::new(9_000, 4, u64::MAX);
        slow.observe(theirs);
        assert!(slow.tick_at(10) > theirs);
    }

    #[test]
    fn serializes_as_a_triple() {
        let h = Hlc::new(1_700_000_000_123, 2, u64::MAX);
        let json = serde_json::to_string(&h).unwrap();
        assert_eq!(json, format!("[1700000000123,2,{}]", u64::MAX));
        assert_eq!(serde_json::from_str::<Hlc>(&json).unwrap(), h);
    }
}
