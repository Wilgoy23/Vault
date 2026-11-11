//! A collection of records keyed by id, with deletes that sync.

use std::collections::{btree_map, BTreeMap};
use std::marker::PhantomData;

use crate::merge::Merge;

/// A record that can live in a [`RecordSet`]: anything that merges and can
/// say when it was last edited, so a delete can be weighed against edits.
pub trait Record: Merge {
    type Clock: Ord + Clone;

    /// The clock of the newest write to any field. `None` if never written.
    fn last_edit(&self) -> Option<Self::Clock>;
}

/// What happens when one device deletes a record that another edited.
pub trait DeletePolicy {
    const DELETE_WINS: bool;
}

/// A delete hides every edit made before it; an edit made after it brings
/// the record back. The record's data is kept (hidden) so all devices agree
/// on what comes back: clear sensitive fields with ordinary writes at the
/// delete's clock before deleting. Vault's current behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditRevives;

impl DeletePolicy for EditRevives {
    const DELETE_WINS: bool = false;
}

/// A delete is final: the record and its data are dropped on every device,
/// whatever was edited before or after.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeleteWins;

impl DeletePolicy for DeleteWins {
    const DELETE_WINS: bool = true;
}

/// Records of type `T` keyed by `K`, plus a tombstone for each deleted id.
/// Tombstones are kept indefinitely: dropping one would let a device that
/// was offline when the delete happened bring the record back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordSet<K, T: Record, P = EditRevives> {
    records: BTreeMap<K, T>,
    deleted: BTreeMap<K, T::Clock>,
    policy: PhantomData<P>,
}

impl<K, T: Record, P> Default for RecordSet<K, T, P> {
    fn default() -> Self {
        RecordSet { records: BTreeMap::new(), deleted: BTreeMap::new(), policy: PhantomData }
    }
}

impl<K: Ord + Clone, T: Record, P: DeletePolicy> RecordSet<K, T, P> {
    pub fn new() -> Self {
        Self::default()
    }

    fn visible(&self, id: &K, record: &T) -> bool {
        match self.deleted.get(id) {
            None => true,
            Some(deleted_at) => {
                !P::DELETE_WINS && record.last_edit().is_some_and(|edit| edit > *deleted_at)
            }
        }
    }

    /// Merges `record` into the one stored under `id`, or adds it. Under
    /// [`EditRevives`] this is also how a deleted record is brought back: by
    /// writing to it after the delete.
    pub fn upsert(&mut self, id: K, record: T) {
        if P::DELETE_WINS && self.deleted.contains_key(&id) {
            return;
        }
        match self.records.entry(id) {
            btree_map::Entry::Occupied(mut e) => e.get_mut().merge(record),
            btree_map::Entry::Vacant(e) => {
                e.insert(record);
            }
        }
    }

    /// The record under `id`, unless it is deleted.
    pub fn get(&self, id: &K) -> Option<&T> {
        self.records.get(id).filter(|r| self.visible(id, r))
    }

    /// The record under `id` for editing, unless it is deleted.
    pub fn get_mut(&mut self, id: &K) -> Option<&mut T> {
        let visible = self.records.get(id).is_some_and(|r| self.visible(id, r));
        if visible {
            self.records.get_mut(id)
        } else {
            None
        }
    }

    /// Deletes `id` as of `clock`.
    pub fn delete(&mut self, id: K, clock: T::Clock) {
        if P::DELETE_WINS {
            self.records.remove(&id);
        }
        let at = self.deleted.entry(id).or_insert_with(|| clock.clone());
        if clock > *at {
            *at = clock;
        }
    }

    pub fn is_deleted(&self, id: &K) -> bool {
        self.deleted.contains_key(id) && self.get(id).is_none()
    }

    /// The records that are not deleted, by id.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)> {
        self.records.iter().filter(|(id, r)| self.visible(id, r))
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<K: Ord, T: Record, P: DeletePolicy> Merge for RecordSet<K, T, P> {
    fn merge(&mut self, other: Self) {
        for (id, clock) in other.deleted {
            let at = self.deleted.entry(id).or_insert_with(|| clock.clone());
            if clock > *at {
                *at = clock;
            }
        }
        Merge::merge(&mut self.records, other.records);
        if P::DELETE_WINS {
            let deleted = &self.deleted;
            self.records.retain(|id, _| !deleted.contains_key(id));
        }
    }
}
