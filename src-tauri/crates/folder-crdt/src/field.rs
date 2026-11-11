//! Field types: the building blocks of a record. Each one is a join on its
//! own, so a record whose fields are all field types merges correctly by
//! merging field by field.
//!
//! Types that take a clock `C` order writes by it. Use [`Hlc`](crate::Hlc):
//! any `Ord` type works, but only a clock that never repeats across devices
//! keeps two writes from tying.
//!
//! All of them serialize with serde (a clock-ordered type as its writes,
//! clock first) and implement [`Zeroize`], wiping every value they hold.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use zeroize::Zeroize;

use crate::merge::Merge;

/// Identifies a device. Random, created once per installation.
pub type DeviceId = u64;

/// A unique stamp for one add: the device that made it and that device's
/// own sequence number. A device must never reuse a sequence number.
/// Serialized as `[device, seq]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Zeroize)]
#[serde(from = "(DeviceId, u64)", into = "(DeviceId, u64)")]
pub struct Tag {
    pub device: DeviceId,
    pub seq: u64,
}

impl Tag {
    pub fn new(device: DeviceId, seq: u64) -> Self {
        Tag { device, seq }
    }
}

impl From<(DeviceId, u64)> for Tag {
    fn from((device, seq): (DeviceId, u64)) -> Self {
        Tag { device, seq }
    }
}

impl From<Tag> for (DeviceId, u64) {
    fn from(t: Tag) -> Self {
        (t.device, t.seq)
    }
}

// ── Lww ───────────────────────────────────────────────────────────────────────

/// Last-writer-wins register: holds the value with the highest clock. Two
/// writes with the same clock are ordered by value, so every device picks
/// the same winner. Empty until first written.
///
/// Serialized as `[clock, value]`, or `null` when empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
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

impl<T: Zeroize, C: Zeroize> Zeroize for Lww<T, C> {
    fn zeroize(&mut self) {
        if let Some((c, v)) = &mut self.write {
            c.zeroize();
            v.zeroize();
        }
        self.write = None;
    }
}

// ── LwwHistory ────────────────────────────────────────────────────────────────

/// A last-writer-wins register that also keeps the `N - 1` values it held
/// before, newest first: every write any device made, not only the ones this
/// device saw. Two devices that change it at once both keep the value that
/// lost. Vault uses it for passwords.
///
/// Serialized as its writes, oldest first, each `[clock, value]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
#[serde(bound(serialize = "T: Serialize, C: Serialize", deserialize = "T: Deserialize<'de>, C: Deserialize<'de>"))]
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

impl<T: Ord + Zeroize, C: Ord + Zeroize, const N: usize> Zeroize for LwwHistory<T, C, N> {
    fn zeroize(&mut self) {
        for (mut c, mut v) in std::mem::take(&mut self.writes.items) {
            c.zeroize();
            v.zeroize();
        }
    }
}

// ── Max ───────────────────────────────────────────────────────────────────────

/// Keeps the largest value ever written: "last used at", a high-water mark.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
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

impl<T: Zeroize> Zeroize for Max<T> {
    fn zeroize(&mut self) {
        self.0.zeroize();
    }
}

// ── Immutable ─────────────────────────────────────────────────────────────────

/// A value set once, when the record is created, and never changed: a
/// creation time, a kind. Copies normally agree; if they don't (two devices
/// created the same id), the smaller value wins everywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Immutable<T>(Option<T>);

impl<T> Default for Immutable<T> {
    fn default() -> Self {
        Immutable(None)
    }
}

impl<T: Ord> Immutable<T> {
    pub fn new(value: T) -> Self {
        Immutable(Some(value))
    }

    pub fn get(&self) -> Option<&T> {
        self.0.as_ref()
    }
}

impl<T: Ord> Merge for Immutable<T> {
    fn merge(&mut self, other: Self) {
        let Some(theirs) = other.0 else { return };
        if self.0.as_ref().is_none_or(|ours| theirs < *ours) {
            self.0 = Some(theirs);
        }
    }
}

impl<T: Zeroize> Zeroize for Immutable<T> {
    fn zeroize(&mut self) {
        self.0.zeroize();
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

/// Serialized as `[[device, count], ...]`: a JSON object would need string
/// keys, which serde can't turn back into numbers inside a flattened struct.
impl Serialize for Counter {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.by_device.iter())
    }
}

impl<'de> Deserialize<'de> for Counter {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let mut c = Counter::default();
        for (device, n) in Vec::<(DeviceId, u64)>::deserialize(d)? {
            c.merge(Counter { by_device: BTreeMap::from([(device, n)]) });
        }
        Ok(c)
    }
}

impl Zeroize for Counter {
    fn zeroize(&mut self) {
        self.by_device.values_mut().for_each(Zeroize::zeroize);
        self.by_device.clear();
    }
}

// ── BoundedSet ────────────────────────────────────────────────────────────────

/// A grow-only set that keeps only its `N` greatest elements. Keeping the top
/// `N` of a union is itself a join, so the cap never makes devices disagree.
/// Order elements so the ones to keep sort last: `(clock, value)` keeps the
/// newest.
///
/// Serialized as its elements in ascending order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
#[serde(bound(serialize = "T: Serialize"))]
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

/// Trims to `N` on the way in, so a file with too many elements still loads.
impl<'de, T: Ord + Deserialize<'de>, const N: usize> Deserialize<'de> for BoundedSet<T, N> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(BTreeSet::<T>::deserialize(d)?.into_iter().collect())
    }
}

impl<T: Ord + Zeroize, const N: usize> Zeroize for BoundedSet<T, N> {
    fn zeroize(&mut self) {
        for mut v in std::mem::take(&mut self.items) {
            v.zeroize();
        }
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

/// Serialized as `{"adds": [[value, [tag, ...]], ...], "removed": [tag, ...]}`,
/// so values need not be strings.
impl<T: Ord + Serialize> Serialize for AddWinsSet<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Repr<'a, T> {
            adds: Vec<(&'a T, &'a BTreeSet<Tag>)>,
            removed: &'a BTreeSet<Tag>,
        }
        Repr { adds: self.adds.iter().collect(), removed: &self.removed }.serialize(s)
    }
}

impl<'de, T: Ord + Deserialize<'de>> Deserialize<'de> for AddWinsSet<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Repr<T> {
            adds: Vec<(T, BTreeSet<Tag>)>,
            removed: BTreeSet<Tag>,
        }
        let r = Repr::<T>::deserialize(d)?;
        let mut set = AddWinsSet { adds: BTreeMap::new(), removed: r.removed };
        for (value, tags) in r.adds {
            set.merge(AddWinsSet { adds: BTreeMap::from([(value, tags)]), removed: BTreeSet::new() });
        }
        Ok(set)
    }
}

impl<T: Ord + Zeroize> Zeroize for AddWinsSet<T> {
    fn zeroize(&mut self) {
        for (mut v, _) in std::mem::take(&mut self.adds) {
            v.zeroize();
        }
        self.removed.clear();
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
