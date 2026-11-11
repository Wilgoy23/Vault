//! The one operation every syncable type has.

use std::collections::{btree_map, BTreeMap, BTreeSet};
use std::fmt::Debug;

/// A type whose copies on different devices can be combined into one.
///
/// `merge` must be a *join*: for any three states `a`, `b` and `c`,
///
/// - `a ⊔ b == b ⊔ a` (commutative): the order copies arrive in doesn't matter;
/// - `(a ⊔ b) ⊔ c == a ⊔ (b ⊔ c)` (associative): neither does how they're grouped;
/// - `a ⊔ a == a` (idempotent): merging a copy twice changes nothing.
///
/// Together these guarantee that devices which have seen the same edits hold
/// the same state, however a sync client interleaves the files. Use
/// [`check_laws`] in a property test to verify an implementation.
pub trait Merge {
    /// Folds `other` into `self`.
    fn merge(&mut self, other: Self);
}

/// `a ⊔ b`, by value.
pub fn merged<T: Merge>(mut a: T, b: T) -> T {
    a.merge(b);
    a
}

/// Checks the three merge laws on one triple of states, comparing results
/// through `view` (often the identity, or a canonical form when the type's
/// own equality is stricter than "same contents"). Returns a description of
/// the first law broken.
pub fn check_laws<T, V>(a: &T, b: &T, c: &T, view: impl Fn(&T) -> V) -> Result<(), String>
where
    T: Merge + Clone,
    V: PartialEq + Debug,
{
    let ab = merged(a.clone(), b.clone());
    let ba = merged(b.clone(), a.clone());
    if view(&ab) != view(&ba) {
        return Err(format!(
            "not commutative\n  a ⊔ b = {:?}\n  b ⊔ a = {:?}",
            view(&ab),
            view(&ba)
        ));
    }

    let ab_c = merged(ab, c.clone());
    let a_bc = merged(a.clone(), merged(b.clone(), c.clone()));
    if view(&ab_c) != view(&a_bc) {
        return Err(format!(
            "not associative\n  (a ⊔ b) ⊔ c = {:?}\n  a ⊔ (b ⊔ c) = {:?}",
            view(&ab_c),
            view(&a_bc)
        ));
    }

    let aa = merged(a.clone(), a.clone());
    if view(&aa) != view(a) {
        return Err(format!(
            "not idempotent\n  a     = {:?}\n  a ⊔ a = {:?}",
            view(a),
            view(&aa)
        ));
    }
    Ok(())
}

/// Maps merge key by key; a key on one side only is kept as is.
impl<K: Ord, V: Merge> Merge for BTreeMap<K, V> {
    fn merge(&mut self, other: Self) {
        for (k, v) in other {
            match self.entry(k) {
                btree_map::Entry::Occupied(mut e) => e.get_mut().merge(v),
                btree_map::Entry::Vacant(e) => {
                    e.insert(v);
                }
            }
        }
    }
}

/// Grow-only set: union.
impl<T: Ord> Merge for BTreeSet<T> {
    fn merge(&mut self, mut other: Self) {
        self.append(&mut other);
    }
}

/// `None` is the empty state: merging with it changes nothing.
impl<T: Merge> Merge for Option<T> {
    fn merge(&mut self, other: Self) {
        if let Some(o) = other {
            match self {
                Some(s) => s.merge(o),
                None => *self = Some(o),
            }
        }
    }
}
