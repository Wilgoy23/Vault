//! A collection of records keyed by id, with deletes that sync.

use std::borrow::Borrow;
use std::collections::{btree_map, BTreeMap};
use std::marker::PhantomData;

use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::field::{AddWinsSet, BoundedSet, Counter, Immutable, Lww, LwwHistory, Max};
use crate::merge::Merge;
use crate::Unknown;

/// A record that can live in a [`RecordSet`]: anything that merges and can
/// say when it was last edited, so a delete can be weighed against edits.
///
/// A struct of field types derives it, with [`Merge`]:
///
/// ```
/// use folder_crdt::{Counter, Hlc, Lww, Merge, Record};
///
/// #[derive(Default, Merge, Record)]
/// #[record(clock = Hlc)]
/// struct Bookmark {
///     title: Lww<String, Hlc>,
///     // Opening a bookmark isn't an edit
///     #[record(skip)]
///     opened: Lww<u64, Hlc>,
///     visits: Counter,
/// }
///
/// let mut b = Bookmark::default();
/// b.title.set("Docs".into(), Hlc::new(1_000, 0, 1));
/// b.opened.set(5, Hlc::new(9_000, 0, 1));
/// b.visits.increment(1, 1);
/// assert_eq!(b.last_edit(), Some(Hlc::new(1_000, 0, 1)));
/// ```
pub trait Record: Merge {
    type Clock: Ord + Clone;

    /// The clock of the newest write to any field. `None` if never written.
    fn last_edit(&self) -> Option<Self::Clock>;
}

/// What a field contributes to its record's [`Record::last_edit`]:
/// `#[derive(Record)]` takes the newest clock its fields report.
///
/// Registers report the clock of their current write. Types with no clock
/// of their own, or whose changes aren't edits (counters, maxima, sets,
/// immutable values, unknown fields), report `None` for every clock type.
pub trait LastEdit<C> {
    fn last_edit(&self) -> Option<C>;
}

impl<C, F: LastEdit<C>> LastEdit<C> for Option<F> {
    fn last_edit(&self) -> Option<C> {
        self.as_ref().and_then(F::last_edit)
    }
}

impl<C: Ord + Clone, T: Ord> LastEdit<C> for Lww<T, C> {
    fn last_edit(&self) -> Option<C> {
        self.clock().cloned()
    }
}

impl<C: Ord + Clone, T: Ord, const N: usize> LastEdit<C> for LwwHistory<T, C, N> {
    fn last_edit(&self) -> Option<C> {
        self.clock().cloned()
    }
}

/// No clock: these report `None` whatever the record's clock is.
macro_rules! unclocked {
    ($([$($g:tt)*] $ty:ty;)*) => {$(
        impl<C, $($g)*> LastEdit<C> for $ty {
            fn last_edit(&self) -> Option<C> {
                None
            }
        }
    )*};
}

unclocked! {
    [T] Max<T>;
    [T] Immutable<T>;
    [] Counter;
    [T: Ord, const N: usize] BoundedSet<T, N>;
    [T: Ord] AddWinsSet<T>;
    [] Unknown;
}

/// What happens when one device deletes a record that another edited.
pub trait DeletePolicy {
    const DELETE_WINS: bool;
}

/// A delete hides every edit made before it; an edit made after it brings
/// the record back. The record's data is kept (hidden) so all devices agree
/// on what comes back: clear sensitive fields with ordinary writes at the
/// delete's clock before deleting. Vault's behaviour up to format 1.
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
///
/// Serialized as `{"records": {id: record}, "deleted": {id: clock}}`. In
/// JSON, ids must be strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(bound(
    serialize = "K: Serialize, T: Serialize, T::Clock: Serialize",
    deserialize = "K: Ord + Deserialize<'de>, T: Deserialize<'de>, T::Clock: Deserialize<'de>"
))]
pub struct RecordSet<K, T: Record, P = EditRevives> {
    records: BTreeMap<K, T>,
    #[serde(default)]
    deleted: BTreeMap<K, T::Clock>,
    #[serde(skip)]
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

    fn visible<Q>(deleted: &BTreeMap<K, T::Clock>, id: &Q, record: &T) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        match deleted.get(id) {
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
    pub fn get<Q>(&self, id: &Q) -> Option<&T>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.records.get(id).filter(|r| Self::visible(&self.deleted, id, r))
    }

    /// The record under `id` for editing, unless it is deleted.
    pub fn get_mut<Q>(&mut self, id: &Q) -> Option<&mut T>
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        let deleted = &self.deleted;
        self.records.get_mut(id).filter(|r| Self::visible(deleted, id, r))
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

    pub fn is_deleted<Q>(&self, id: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.deleted.contains_key(id) && self.get(id).is_none()
    }

    /// The records that are not deleted, by id.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &T)> {
        self.records.iter().filter(|(id, r)| Self::visible(&self.deleted, *id, r))
    }

    /// The records that are not deleted, by id, for editing.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut T)> {
        let deleted = &self.deleted;
        self.records.iter_mut().filter(move |(id, r)| Self::visible(deleted, *id, r))
    }

    pub fn len(&self) -> usize {
        self.iter().count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The newest clock anywhere in the set, deleted records included: what
    /// a device's clock should [observe](crate::HlcClock::observe) after
    /// loading or merging it.
    pub fn latest_clock(&self) -> Option<T::Clock> {
        let edits = self.records.values().filter_map(Record::last_edit);
        edits.chain(self.deleted.values().cloned()).max()
    }
}

impl<K: Zeroize, T: Record + Zeroize, P> Zeroize for RecordSet<K, T, P>
where
    T::Clock: Zeroize,
{
    fn zeroize(&mut self) {
        for (mut id, mut record) in std::mem::take(&mut self.records) {
            id.zeroize();
            record.zeroize();
        }
        for (mut id, mut clock) in std::mem::take(&mut self.deleted) {
            id.zeroize();
            clock.zeroize();
        }
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
