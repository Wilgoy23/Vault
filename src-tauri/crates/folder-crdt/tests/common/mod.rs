//! Shared harness for the merge-law property tests.
//!
//! States are built the way real devices build them: device 0 makes a
//! shared history, then devices 1, 2 and 3 each copy it and make their own
//! edits offline. The laws are checked on the three diverged copies.

#![allow(dead_code)]

use folder_crdt::{DeviceId, Tag};
use proptest::test_runner::Config;

/// Cases per property. The PRD's gate: 10,000 per law per run.
pub const CASES: u32 = 10_000;

pub fn config() -> Config {
    Config { cases: CASES, ..Config::default() }
}

/// One device's own counters.
pub struct Device {
    pub id: DeviceId,
    seq: u64,
    pub clock: u64,
}

impl Device {
    /// A fresh tag, never used by any other add on any device.
    pub fn tag(&mut self) -> Tag {
        self.seq += 1;
        Tag::new(self.id, self.seq)
    }

    /// Moves this device's clock forward by `dt` and returns it. A `dt` of 0
    /// gives two edits the same time, as two edits in one second do today.
    pub fn tick(&mut self, dt: u64) -> u64 {
        self.clock += dt;
        self.clock
    }
}

/// Applies `base` on device 0, then each of `a`, `b`, `c` on its own copy.
pub fn replicas<S: Clone + Default, O>(
    base: &[O],
    a: &[O],
    b: &[O],
    c: &[O],
    apply: impl Fn(&mut S, &mut Device, &O),
) -> (S, S, S) {
    let mut shared = S::default();
    let mut dev0 = Device { id: 0, seq: 0, clock: 0 };
    for op in base {
        apply(&mut shared, &mut dev0, op);
    }
    let run = |id: DeviceId, ops: &[O]| {
        let mut state = shared.clone();
        let mut dev = Device { id, seq: 0, clock: dev0.clock };
        for op in ops {
            apply(&mut state, &mut dev, op);
        }
        state
    };
    (run(1, a), run(2, b), run(3, c))
}
