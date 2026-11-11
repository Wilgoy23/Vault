//! `#[derive(Merge, Record)]` on the shapes an app might write: generic,
//! tuple and unit structs, skipped and optional fields, lists, and a struct
//! that wipes itself on drop. Vault's own types use the derives too, and
//! pass `vault_laws` and `vault` unchanged.

mod common;

use common::{config, replicas, Device};
use folder_crdt::*;
use proptest::collection::vec;
use proptest::prelude::*;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// Generic over its value and clock, with a register that doesn't count as
/// an edit, an optional register, and fields with no clock.
#[derive(Debug, Clone, PartialEq, Eq, Merge, Record)]
#[record(clock = C)]
struct Card<T: Ord, C: Ord + Clone> {
    front: Lww<T, C>,
    back: Option<Lww<T, C>>,
    #[record(skip)]
    reviewed: Lww<T, C>,
    reviews: Counter,
    tags: AddWinsSet<T>,
}

// Not derived: `Default` on a generic struct would want `T: Default`.
impl<T: Ord, C: Ord + Clone> Default for Card<T, C> {
    fn default() -> Self {
        Card {
            front: Lww::default(),
            back: None,
            reviewed: Lww::default(),
            reviews: Counter::default(),
            tags: AddWinsSet::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Merge, Record)]
#[record(clock = u64)]
struct Pair(Lww<u8, u64>, Max<u64>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Merge, Record)]
#[record(clock = u64)]
struct Nothing;

#[derive(Debug, Clone, Default, PartialEq, Eq, Merge, Record)]
#[record(clock = u64)]
struct Checklist {
    title: Lww<String, u64>,
    steps: Sequence<String, u64>,
}

/// Wiped on drop, so the derive can't move fields out of `other`.
#[derive(Clone, Default, Zeroize, ZeroizeOnDrop, Merge)]
struct Wiped {
    secret: Lww<String, u64>,
    uses: Counter,
}

#[test]
fn last_edit_is_the_newest_counted_register() {
    let mut card = Card::<&str, u64>::default();
    assert_eq!(card.last_edit(), None);

    card.front.set("a", 3);
    card.reviewed.set("seen", 9);
    card.reviews.increment(1, 1);
    card.tags.add("t", Tag::new(1, 1));
    assert_eq!(card.last_edit(), Some(3), "skipped and unclocked fields don't count");

    card.back = Some(Lww::new("b", 5));
    assert_eq!(card.last_edit(), Some(5), "an optional register counts when present");
}

#[test]
fn tuple_and_unit_structs() {
    let mut a = Pair(Lww::new(1, 1), Max::new(7));
    a.merge(Pair(Lww::new(2, 2), Max::new(4)));
    assert_eq!(a, Pair(Lww::new(2, 2), Max::new(7)));
    assert_eq!(a.last_edit(), Some(2), "a maximum isn't an edit");

    let mut n = Nothing;
    n.merge(Nothing);
    assert_eq!(n.last_edit(), None);
}

#[test]
fn a_list_reports_its_newest_item_write() {
    let mut list = Checklist::default();
    list.title.set("Trip".into(), 1);
    list.steps.insert(0, Tag::new(1, 1), "pack".into(), 4);
    assert_eq!(list.last_edit(), Some(4));
}

#[test]
fn a_struct_that_wipes_on_drop_merges() {
    let mut a = Wiped::default();
    a.secret.set("old".into(), 1);
    a.uses.increment(1, 1);
    let mut b = Wiped::default();
    b.secret.set("new".into(), 2);
    b.uses.increment(2, 3);
    a.merge(b);
    assert_eq!(a.secret.get().map(String::as_str), Some("new"));
    assert_eq!(a.uses.value(), 4);
}

#[test]
fn derived_records_work_in_a_record_set() {
    let mut a: RecordSet<String, Card<u8, u64>, DeleteWins> = RecordSet::default();
    let mut b = a.clone();
    a.upsert("1".into(), Card { front: Lww::new(1, 1), ..Card::default() });
    b.upsert("1".into(), Card { front: Lww::new(2, 2), ..Card::default() });
    b.delete("1".into(), 3);
    a.merge(b);
    assert!(a.get("1").is_none(), "the delete at 3 beats the edits before it");
}

// ── Merge laws on a derived generic record ───────────────────────────────────

#[derive(Debug, Clone)]
enum Op {
    Front(u8, u64),
    Back(u8, u64),
    Review(u8, u64),
    Count,
    Tag(u8),
}

fn op() -> impl Strategy<Value = Op> {
    let v = 0u8..4;
    let dt = 0u64..3;
    prop_oneof![
        (v.clone(), dt.clone()).prop_map(|(v, d)| Op::Front(v, d)),
        (v.clone(), dt.clone()).prop_map(|(v, d)| Op::Back(v, d)),
        (v.clone(), dt).prop_map(|(v, d)| Op::Review(v, d)),
        Just(Op::Count),
        v.prop_map(Op::Tag),
    ]
}

fn apply(card: &mut Card<u8, u64>, d: &mut Device, op: &Op) {
    match *op {
        Op::Front(v, dt) => card.front.set(v, d.tick(dt)),
        Op::Back(v, dt) => {
            let at = d.tick(dt);
            card.back.get_or_insert_with(Lww::default).set(v, at)
        }
        Op::Review(v, dt) => card.reviewed.set(v, d.tick(dt)),
        Op::Count => card.reviews.increment(d.id, 1),
        Op::Tag(v) => {
            let tag = d.tag();
            card.tags.add(v, tag)
        }
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn derived_merge_is_a_join(
        base in vec(op(), 0..8),
        a in vec(op(), 0..8),
        b in vec(op(), 0..8),
        c in vec(op(), 0..8),
    ) {
        let (a, b, c) = replicas::<Card<u8, u64>, _>(&base, &a, &b, &c, apply);
        check_laws(&a, &b, &c, |s| s.clone()).map_err(TestCaseError::fail)?;
        // The merge's last edit is the newest of the copies'
        let newest = [&a, &b, &c].iter().filter_map(|s| s.last_edit()).max();
        prop_assert_eq!(merged(merged(a, b), c).last_edit(), newest);
    }
}
