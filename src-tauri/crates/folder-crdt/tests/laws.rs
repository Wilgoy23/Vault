//! The merge laws for every generic type in the crate.

mod common;

use common::{config, replicas, Device};
use folder_crdt::*;
use proptest::collection::vec;
use proptest::prelude::*;

/// A property test: random histories on a base and three devices, then the
/// three laws on the results.
macro_rules! laws {
    ($name:ident, $state:ty, $op:expr, $apply:expr) => {
        proptest! {
            #![proptest_config(config())]
            #[test]
            fn $name(
                base in vec($op, 0..8),
                a in vec($op, 0..8),
                b in vec($op, 0..8),
                c in vec($op, 0..8),
            ) {
                let (a, b, c) = replicas::<$state, _>(&base, &a, &b, &c, $apply);
                check_laws(&a, &b, &c, |s| s.clone()).map_err(TestCaseError::fail)?;
            }
        }
    };
}

/// A value from a small pool, so devices often write the same one.
fn val() -> impl Strategy<Value = u8> {
    0u8..4
}

/// Clock steps of 0 to 2: ties are common, as they are with unix seconds.
fn dt() -> impl Strategy<Value = u64> {
    0u64..3
}

// ── Field types ───────────────────────────────────────────────────────────────

laws!(lww, Lww<u8, u64>, (val(), dt()), |s: &mut Lww<u8, u64>, d: &mut Device, (v, dt): &(u8, u64)| {
    s.set(*v, d.tick(*dt))
});

laws!(
    lww_history,
    LwwHistory<u8, u64, 3>,
    (val(), dt()),
    |s: &mut LwwHistory<u8, u64, 3>, d: &mut Device, (v, dt): &(u8, u64)| s.set(*v, d.tick(*dt))
);

laws!(max, Max<u8>, val(), |s: &mut Max<u8>, _: &mut Device, v: &u8| s.set(*v));

laws!(counter, Counter, 0u64..3, |s: &mut Counter, d: &mut Device, by: &u64| s.increment(d.id, *by));

laws!(
    bounded_set,
    BoundedSet<(u64, u8), 3>,
    (val(), dt()),
    |s: &mut BoundedSet<(u64, u8), 3>, d: &mut Device, (v, dt): &(u8, u64)| s.insert((d.tick(*dt), *v))
);

#[derive(Debug, Clone)]
enum SetOp {
    Add(u8),
    Remove(u8),
}

fn set_op() -> impl Strategy<Value = SetOp> {
    prop_oneof![val().prop_map(SetOp::Add), val().prop_map(SetOp::Remove)]
}

laws!(add_wins_set, AddWinsSet<u8>, set_op(), |s: &mut AddWinsSet<u8>, d: &mut Device, op: &SetOp| {
    match op {
        SetOp::Add(v) => s.add(*v, d.tag()),
        SetOp::Remove(v) => s.remove(v),
    }
});

// ── Sequence ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum SeqOp {
    Insert { index: usize, v: u8, dt: u64 },
    Move { pick: usize, index: usize, dt: u64 },
    Set { pick: usize, v: u8, dt: u64 },
    Remove { pick: usize },
}

fn seq_op() -> impl Strategy<Value = SeqOp> {
    prop_oneof![
        (0usize..6, val(), dt()).prop_map(|(index, v, dt)| SeqOp::Insert { index, v, dt }),
        (0usize..6, 0usize..6, dt()).prop_map(|(pick, index, dt)| SeqOp::Move { pick, index, dt }),
        (0usize..6, val(), dt()).prop_map(|(pick, v, dt)| SeqOp::Set { pick, v, dt }),
        (0usize..6).prop_map(|pick| SeqOp::Remove { pick }),
    ]
}

/// The id of the `pick`th item in this device's view, if any.
fn nth_item(s: &Sequence<u8, u64>, pick: usize) -> Option<ItemId> {
    let len = s.len();
    (len > 0).then(|| *s.iter().nth(pick % len).unwrap().0)
}

laws!(sequence, Sequence<u8, u64>, seq_op(), |s: &mut Sequence<u8, u64>, d: &mut Device, op: &SeqOp| {
    match *op {
        SeqOp::Insert { index, v, dt } => {
            let (id, clock) = (d.tag(), d.tick(dt));
            s.insert(index, id, v, clock);
        }
        SeqOp::Move { pick, index, dt } => {
            if let Some(id) = nth_item(s, pick) {
                s.move_to(&id, index, d.tick(dt));
            }
        }
        SeqOp::Set { pick, v, dt } => {
            if let Some(id) = nth_item(s, pick) {
                s.set(&id, v, d.tick(dt));
            }
        }
        SeqOp::Remove { pick } => {
            if let Some(id) = nth_item(s, pick) {
                s.remove(&id);
            }
        }
    }
});

proptest! {
    #![proptest_config(config())]

    /// A position made between two neighbours always sorts between them.
    #[test]
    fn position_between_is_strictly_inside(cuts in vec(0usize..4, 1..20)) {
        // Build positions only the way the crate does, by repeated splitting
        let mut points = vec![Position::between(None, None)];
        for cut in cuts {
            let i = cut % (points.len() + 1);
            let lo = i.checked_sub(1).map(|j| &points[j]);
            let hi = points.get(i);
            let mid = Position::between(lo, hi);
            prop_assert!(lo.is_none_or(|l| *l < mid));
            prop_assert!(hi.is_none_or(|h| mid < *h));
            points.insert(i, mid);
        }
    }
}

// ── RecordSet ─────────────────────────────────────────────────────────────────

/// A small record of two field types.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Note {
    title: Lww<u8, u64>,
    tags: AddWinsSet<u8>,
}

impl Merge for Note {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title);
        self.tags.merge(other.tags);
    }
}

impl Record for Note {
    type Clock = u64;
    fn last_edit(&self) -> Option<u64> {
        self.title.clock().copied()
    }
}

#[derive(Debug, Clone)]
enum NoteOp {
    Write { id: u8, title: u8, dt: u64 },
    Tag { id: u8, tag: u8 },
    Delete { id: u8, dt: u64 },
}

fn note_op() -> impl Strategy<Value = NoteOp> {
    prop_oneof![
        (0u8..3, val(), dt()).prop_map(|(id, title, dt)| NoteOp::Write { id, title, dt }),
        (0u8..3, val()).prop_map(|(id, tag)| NoteOp::Tag { id, tag }),
        (0u8..3, dt()).prop_map(|(id, dt)| NoteOp::Delete { id, dt }),
    ]
}

fn apply_note<P: DeletePolicy>(s: &mut RecordSet<u8, Note, P>, d: &mut Device, op: &NoteOp) {
    match *op {
        NoteOp::Write { id, title, dt } => {
            let note = Note { title: Lww::new(title, d.tick(dt)), ..Note::default() };
            s.upsert(id, note);
        }
        NoteOp::Tag { id, tag } => {
            let t = d.tag();
            if let Some(note) = s.get_mut(&id) {
                note.tags.add(tag, t);
            }
        }
        NoteOp::Delete { id, dt } => s.delete(id, d.tick(dt)),
    }
}

laws!(record_set_edit_revives, RecordSet<u8, Note, EditRevives>, note_op(), apply_note::<EditRevives>);
laws!(record_set_delete_wins, RecordSet<u8, Note, DeleteWins>, note_op(), apply_note::<DeleteWins>);
