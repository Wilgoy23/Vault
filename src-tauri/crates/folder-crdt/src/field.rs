//! Field types: the building blocks of a record. Each one is a join on its
//! own, so a record whose fields are all field types merges correctly by
//! merging field by field.
//!
//! Types that take a clock `C` order writes by it. Until phase 3 brings the
//! hybrid logical clock, any `Ord` type works; Vault's current data uses
//! unix seconds.

use std::collections::{BTreeMap, BTreeSet};

use crate::merge::Merge;

/// Identifies a device. Random, created once per installation.
pub type DeviceId = u64;

/// A unique stamp for one add: the device that made it and that device's
/// own sequence number. A device must never reuse a sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag {
    pub device: DeviceId,
    pub seq: u64,
}

impl Tag {
    pub fn new(device: DeviceId, seq: u64) -> Self {
        Tag { device, seq }
    }
}

// ── Lww ───────────────────────────────────────────────────────────────────────

/// Last-writer-wins register: holds the value with the highest clock. Two
/// writes with the same clock are ordered by value, so every device picks
/// the same winner. Empty until first written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lww<T, C> {
    write: Option<(C, T)>,
}

impl<T, C> Default for Lww<T, C> {
    fn default() -> Self {
        Lww { write: None }
    }
}

impl<T: Ord, C: Ord> Lww<T, C> {
    pub fn new(value: T, clock: C) -> Self {
        Lww { write: Some((clock, value)) }
    }

    /// Writes `value` at `clock`. A write older than the current one is
    /// ignored, exactly as if it had arrived from another device.
    pub fn set(&mut self, value: T, clock: C) {
        self.merge(Lww::new(value, clock));
    }

    pub fn get(&self) -> Option<&T> {
        self.write.as_ref().map(|(_, v)| v)
    }

    pub fn clock(&self) -> Option<&C> {
        self.write.as_ref().map(|(c, _)| c)
    }
}

impl<T: Ord, C: Ord> Merge for Lww<T, C> {
    fn merge(&mut self, other: Self) {
        let Some(theirs) = other.write else { return };
        let newer = match &self.write {
            None => true,
            Some(ours) => (&theirs.0, &theirs.1) > (&ours.0, &ours.1),
        };
        if newer {
            self.write = Some(theirs);
        }
    }
}

// ── LwwHistory ────────────────────────────────────────────────────────────────

/// A last-writer-wins register that also keeps the `N - 1` values it held
/// before, newest first: every write any device made, not only the ones this
/// device saw. Two devices that change it at once both keep the value that
/// lost. Vault uses it for passwords.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LwwHistory<T: Ord, C: Ord, const N: usize> {
    writes: BoundedSet<(C, T), N>,
}

impl<T: Ord, C: Ord, const N: usize> Default for LwwHistory<T, C, N> {
    fn default() -> Self {
        LwwHistory { writes: BoundedSet::default() }
    }
}

impl<T: Ord, C: Ord, const N: usize> LwwHistory<T, C, N> {
    pub fn new(value: T, clock: C) -> Self {
        let mut h = Self::default();
        h.set(value, clock);
        h
    }

    pub fn set(&mut self, value: T, clock: C) {
        self.writes.insert((clock, value));
    }

    /// The current value: the newest write.
    pub fn get(&self) -> Option<&T> {
        self.writes.iter().next_back().map(|(_, v)| v)
    }

    pub fn clock(&self) -> Option<&C> {
        self.writes.iter().next_back().map(|(c, _)| c)
    }

    /// Earlier values with the clock each was written at, newest first.
    pub fn history(&self) -> impl Iterator<Item = (&C, &T)> {
        self.writes.iter().rev().skip(1).map(|(c, v)| (c, v))
    }
}

impl<T: Ord, C: Ord, const N: usize> Merge for LwwHistory<T, C, N> {
    fn merge(&mut self, other: Self) {
        self.writes.merge(other.writes);
    }
}

// ── Max ───────────────────────────────────────────────────────────────────────

/// Keeps the largest value ever written: "last used at", a high-water mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Max<T>(Option<T>);

impl<T> Default for Max<T> {
    fn default() -> Self {
        Max(None)
    }
}

impl<T: Ord> Max<T> {
    pub fn new(value: T) -> Self {
        Max(Some(value))
    }

    pub fn set(&mut self, value: T) {
        self.merge(Max::new(value));
    }

    pub fn get(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

impl<T: Ord> Merge for Max<T> {
    fn merge(&mut self, other: Self) {
        if other.0 > self.0 {
            self.0 = other.0;
        }
    }
}

// ── Counter ───────────────────────────────────────────────────────────────────

/// A count that only goes up, kept per device so increments made on two
/// devices at once are both counted. A plain `max` would keep only one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counter {
    by_device: BTreeMap<DeviceId, u64>,
}

impl Counter {
    pub fn increment(&mut self, device: DeviceId, by: u64) {
        let n = self.by_device.entry(device).or_insert(0);
        *n = n.saturating_add(by);
    }

    pub fn value(&self) -> u64 {
        self.by_device.values().fold(0, |sum, n| sum.saturating_add(*n))
    }
}

impl Merge for Counter {
    fn merge(&mut self, other: Self) {
        for (device, n) in other.by_device {
            let ours = self.by_device.entry(device).or_insert(0);
            *ours = (*ours).max(n);
        }
    }
}

// ── BoundedSet ────────────────────────────────────────────────────────────────

/// A grow-only set that keeps only its `N` greatest elements. Keeping the top
/// `N` of a union is itself a join, so the cap never makes devices disagree.
/// Order elements so the ones to keep sort last: `(clock, value)` keeps the
/// newest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedSet<T: Ord, const N: usize> {
    items: BTreeSet<T>,
}

impl<T: Ord, const N: usize> Default for BoundedSet<T, N> {
    fn default() -> Self {
        BoundedSet { items: BTreeSet::new() }
    }
}

impl<T: Ord, const N: usize> BoundedSet<T, N> {
    pub fn insert(&mut self, value: T) {
        self.items.insert(value);
        self.trim();
    }

    /// Ascending: the element that would be dropped next comes first.
    pub fn iter(&self) -> std::collections::btree_set::Iter<'_, T> {
        self.items.iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    fn trim(&mut self) {
        while self.items.len() > N {
            self.items.pop_first();
        }
    }
}

impl<T: Ord, const N: usize> FromIterator<T> for BoundedSet<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut s = BoundedSet { items: iter.into_iter().collect() };
        s.trim();
        s
    }
}

impl<T: Ord, const N: usize> Merge for BoundedSet<T, N> {
    fn merge(&mut self, mut other: Self) {
        self.items.append(&mut other.items);
        self.trim();
    }
}

// ── AddWinsSet ────────────────────────────────────────────────────────────────

/// A set you can add to and remove from on any device. Each add carries a
/// unique [`Tag`]; a remove cancels only the adds it has seen. So when one
/// device removes a tag while another adds it again, the tag stays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddWinsSet<T: Ord> {
    adds: BTreeMap<T, BTreeSet<Tag>>,
    /// Tags of adds that have been cancelled. Kept so a copy that still
    /// holds the add cannot bring it back.
    removed: BTreeSet<Tag>,
}

impl<T: Ord> Default for AddWinsSet<T> {
    fn default() -> Self {
        AddWinsSet { adds: BTreeMap::new(), removed: BTreeSet::new() }
    }
}

impl<T: Ord> AddWinsSet<T> {
    /// Adds `value`. `tag` must not have been used for any other add.
    pub fn add(&mut self, value: T, tag: Tag) {
        if !self.removed.contains(&tag) {
            self.adds.entry(value).or_default().insert(tag);
        }
    }

    /// Removes `value` as this device sees it now.
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.adds.remove(value) {
            self.removed.extend(tags);
        }
    }

    pub fn contains(&self, value: &T) -> bool {
        self.adds.contains_key(value)
    }

    /// The values in the set, in ascending order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.adds.keys()
    }

    pub fn len(&self) -> usize {
        self.adds.len()
    }

    pub fn is_empty(&self) -> bool {
        self.adds.is_empty()
    }
}

impl<T: Ord> Merge for AddWinsSet<T> {
    fn merge(&mut self, mut other: Self) {
        for (value, tags) in other.adds {
            self.adds.entry(value).or_default().extend(tags);
        }
        self.removed.append(&mut other.removed);
        let removed = &self.removed;
        self.adds.retain(|_, tags| {
            tags.retain(|t| !removed.contains(t));
            !tags.is_empty()
        });
    }
}
