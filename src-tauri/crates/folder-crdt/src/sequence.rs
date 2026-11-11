//! An ordered list that several devices can insert into, reorder and remove
//! from at once.
//!
//! Each item has a [`Position`]: a fraction, written as bytes, that sorts
//! between its neighbours. Inserting or moving an item gives it a new
//! position between the items around the spot, held in a last-writer-wins
//! register, so concurrent moves of different items both land and
//! concurrent moves of the same item settle on one. Items with equal
//! positions (two devices inserting at the same spot) are ordered by id.

use std::collections::{BTreeMap, BTreeSet};

use crate::field::{Lww, Tag};
use crate::merge::Merge;

/// Identifies an item in a [`Sequence`]. Unique per insert, like a [`Tag`].
pub type ItemId = Tag;

/// A point on an ordering line between "before everything" and "after
/// everything". Compared byte by byte; never ends in a zero byte, so there
/// is always room for another position between any two.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Position(Vec<u8>);

impl Position {
    /// A position strictly between `lo` and `hi` (`None` = an open end).
    /// If `lo` is not below `hi`, returns one just after `lo`.
    pub fn between(lo: Option<&Position>, hi: Option<&Position>) -> Position {
        let lo: &[u8] = lo.map_or(&[], |p| &p.0);
        let hi: Option<&[u8]> = hi.map(|p| p.0.as_slice());
        if let Some(h) = hi {
            if lo >= h {
                let mut after = lo.to_vec();
                after.push(128);
                return Position(after);
            }
        }

        // Walk the digits (base 256, `lo` padded with zeros, an open `hi`
        // as 256) until there is a gap to put a digit in.
        let mut out = Vec::new();
        let mut hi_bound = hi;
        let mut i = 0;
        loop {
            let a = lo.get(i).copied().unwrap_or(0) as u16;
            let b = match hi_bound {
                Some(h) => h.get(i).copied().unwrap_or(0) as u16,
                None => 256,
            };
            if b > a + 1 {
                out.push(((a + b) / 2) as u8);
                return Position(out);
            }
            out.push(a as u8);
            if b == a + 1 {
                // Below `hi` from here on, whatever follows
                hi_bound = None;
            }
            i += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Item<T, C> {
    value: Lww<T, C>,
    position: Lww<Position, C>,
}

impl<T: Ord, C: Ord> Merge for Item<T, C> {
    fn merge(&mut self, other: Self) {
        self.value.merge(other.value);
        self.position.merge(other.position);
    }
}

/// An ordered list of `T`, written with clocks of type `C`. A removed item
/// stays removed, even if another device moved or edited it at the same time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence<T, C> {
    items: BTreeMap<ItemId, Item<T, C>>,
    removed: BTreeSet<ItemId>,
}

impl<T, C> Default for Sequence<T, C> {
    fn default() -> Self {
        Sequence { items: BTreeMap::new(), removed: BTreeSet::new() }
    }
}

impl<T: Ord, C: Ord> Sequence<T, C> {
    /// Item ids and positions in list order, without `skip`.
    fn order(&self, skip: Option<&ItemId>) -> Vec<(&ItemId, &Position)> {
        let mut order: Vec<_> = self
            .items
            .iter()
            .filter(|(id, _)| Some(*id) != skip)
            .filter_map(|(id, item)| item.position.get().map(|p| (id, p)))
            .collect();
        order.sort_by(|a, b| (a.1, a.0).cmp(&(b.1, b.0)));
        order
    }

    fn position_at(&self, index: usize, skip: Option<&ItemId>) -> Position {
        let order = self.order(skip);
        let index = index.min(order.len());
        let lo = index.checked_sub(1).map(|i| order[i].1);
        let hi = order.get(index).map(|(_, p)| *p);
        Position::between(lo, hi)
    }

    /// Inserts `value` so it lands at `index` in this device's view (the end
    /// if `index` is past it). Does nothing if `id` is already used.
    pub fn insert(&mut self, index: usize, id: ItemId, value: T, clock: C)
    where
        C: Clone,
    {
        if self.items.contains_key(&id) || self.removed.contains(&id) {
            return;
        }
        let position = self.position_at(index, None);
        self.items.insert(
            id,
            Item { value: Lww::new(value, clock.clone()), position: Lww::new(position, clock) },
        );
    }

    /// Moves an item so it lands at `index` in this device's view. Returns
    /// false if there is no such item.
    pub fn move_to(&mut self, id: &ItemId, index: usize, clock: C) -> bool {
        if !self.items.contains_key(id) {
            return false;
        }
        let position = self.position_at(index, Some(id));
        self.items.get_mut(id).map(|item| item.position.set(position, clock)).is_some()
    }

    /// Replaces an item's value. Returns false if there is no such item.
    pub fn set(&mut self, id: &ItemId, value: T, clock: C) -> bool {
        self.items.get_mut(id).map(|item| item.value.set(value, clock)).is_some()
    }

    pub fn remove(&mut self, id: &ItemId) {
        self.items.remove(id);
        self.removed.insert(*id);
    }

    /// The items in list order.
    pub fn iter(&self) -> impl Iterator<Item = (&ItemId, &T)> {
        self.order(None)
            .into_iter()
            .filter_map(|(id, _)| self.items[id].value.get().map(|v| (id, v)))
            .collect::<Vec<_>>()
            .into_iter()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl<T: Ord, C: Ord> Merge for Sequence<T, C> {
    fn merge(&mut self, mut other: Self) {
        Merge::merge(&mut self.items, other.items);
        self.removed.append(&mut other.removed);
        let removed = &self.removed;
        self.items.retain(|id, _| !removed.contains(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(bytes: &[u8]) -> Position {
        Position(bytes.to_vec())
    }

    #[test]
    fn between_lands_strictly_inside() {
        type Bound = Option<&'static [u8]>;
        let cases: &[(Bound, Bound)] = &[
            (None, None),
            (Some(&[128]), None),
            (None, Some(&[128])),
            (Some(&[1]), Some(&[2])),
            (Some(&[255]), None),
            (Some(&[1]), Some(&[1, 1])),
            (Some(&[5, 255, 255]), Some(&[6])),
        ];
        for (lo, hi) in cases {
            let lo = lo.map(pos);
            let hi = hi.map(pos);
            let mid = Position::between(lo.as_ref(), hi.as_ref());
            assert!(lo.as_ref().is_none_or(|l| *l < mid), "{lo:?} < {mid:?}");
            assert!(hi.as_ref().is_none_or(|h| mid < *h), "{mid:?} < {hi:?}");
            assert_ne!(mid.0.last(), Some(&0));
        }
    }

    #[test]
    fn insert_move_and_remove_keep_list_order() {
        let mut s: Sequence<&str, u64> = Sequence::default();
        s.insert(0, Tag::new(1, 1), "b", 1);
        s.insert(0, Tag::new(1, 2), "a", 2);
        s.insert(9, Tag::new(1, 3), "c", 3);
        fn values(s: &Sequence<&'static str, u64>) -> Vec<&'static str> {
            s.iter().map(|(_, v)| *v).collect()
        }
        assert_eq!(values(&s), ["a", "b", "c"]);

        s.move_to(&Tag::new(1, 1), 0, 4);
        assert_eq!(values(&s), ["b", "a", "c"]);
        s.remove(&Tag::new(1, 2));
        assert_eq!(values(&s), ["b", "c"]);
    }
}
