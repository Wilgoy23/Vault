//! The merge laws against Vault's current merge (`folder_crdt::vault`),
//! with each device editing exactly as Vault's own commands do.
//!
//! Both properties are ignored: the current merge breaks the laws in three
//! ways, found by these tests in phase 2. None can be fixed without changing
//! the file format, so the decision is to fix them in phase 3, when entries
//! move to per-field hybrid clocks, `RecordSet` and `LwwHistory`. These tests
//! then become its acceptance test. Run them with `cargo test -- --ignored`.
//!
//! 1. Not associative when a delete races edits on two other devices. The
//!    tombstone drops a copy edited before the delete, but only if it
//!    arrives before a copy edited after it; otherwise the two are already
//!    folded together. A password rotation or use count on the dropped copy
//!    survives on one merge order and is lost on another.
//! 2. Not idempotent when a password changes twice in one second: local
//!    edits append to the history, merges sort it, so merging a copy with
//!    itself reorders it and `sync` rewrites the file once. Nothing is lost.
//! 3. Not associative when two devices rotate a password in the same
//!    second: the tie is broken on the password text, and the losing
//!    password goes into history stamped with the winner's time, which
//!    depends on which merge happened first. The current password can end
//!    up in its own history.
//!
//! Findings 2 and 3 need same-second edits; 1 needs a three-way race around
//! a delete. All three are rare in practice, which is why the existing
//! example tests never hit them.

mod common;

use common::{config, replicas, Device};
use folder_crdt::check_laws;
use folder_crdt::vault::*;
use proptest::collection::vec;
use proptest::prelude::*;

#[derive(Debug, Clone)]
enum Op {
    AddEntry { name: u8, pw: u8, dt: u64 },
    Edit { pick: usize, name: u8, notes: Option<u8>, folder: Option<usize>, dt: u64 },
    Rotate { pick: usize, pw: u8, dt: u64 },
    Delete { pick: usize, dt: u64 },
    Use { pick: usize, dt: u64 },
    AddFolder { name: u8, dt: u64 },
    RenameFolder { pick: usize, name: u8, dt: u64 },
    DeleteFolder { pick: usize, dt: u64 },
}

fn op() -> impl Strategy<Value = Op> {
    let small = || 0u8..3;
    let pick = || 0usize..4;
    let dt = || 0u64..3;
    prop_oneof![
        (small(), small(), dt()).prop_map(|(name, pw, dt)| Op::AddEntry { name, pw, dt }),
        (pick(), small(), proptest::option::of(small()), proptest::option::of(pick()), dt())
            .prop_map(|(pick, name, notes, folder, dt)| Op::Edit { pick, name, notes, folder, dt }),
        (pick(), small(), dt()).prop_map(|(pick, pw, dt)| Op::Rotate { pick, pw, dt }),
        (pick(), dt()).prop_map(|(pick, dt)| Op::Delete { pick, dt }),
        (pick(), dt()).prop_map(|(pick, dt)| Op::Use { pick, dt }),
        (small(), dt()).prop_map(|(name, dt)| Op::AddFolder { name, dt }),
        (pick(), small(), dt()).prop_map(|(pick, name, dt)| Op::RenameFolder { pick, name, dt }),
        (pick(), dt()).prop_map(|(pick, dt)| Op::DeleteFolder { pick, dt }),
    ]
}

fn pick<T>(items: &mut [T], n: usize) -> Option<&mut T> {
    let len = items.len();
    (len > 0).then(move || &mut items[n % len])
}

fn record_deletion(data: &mut VaultData, id: &str, now: u64) {
    data.deleted.retain(|t| t.id != id);
    data.deleted.push(Tombstone { id: id.to_owned(), deleted_at: now });
}

/// One edit, made the way Vault's command for it makes it.
fn apply(data: &mut VaultData, d: &mut Device, op: &Op) {
    match *op {
        Op::AddEntry { name, pw, dt } => {
            let now = d.tick(dt);
            let seq = d.tag().seq;
            let id = format!("e{}-{seq}", d.id);
            data.entries.push(Entry {
                id,
                name: format!("name{name}"),
                kind: EntryKind::Login,
                username: None,
                email: "user@example.com".into(),
                password: format!("pw{pw}"),
                url: None,
                notes: None,
                folder_id: None,
                totp_secret: None,
                created_at: now,
                updated_at: now,
                password_changed_at: Some(now),
                password_history: Vec::new(),
                last_used_at: None,
                use_count: 0,
            });
        }
        Op::Edit { pick: n, name, notes, folder, dt } => {
            let now = d.tick(dt);
            let folder_id = folder.and_then(|f| {
                let len = data.folders.len();
                (len > 0).then(|| data.folders[f % len].id.clone())
            });
            if let Some(e) = pick(&mut data.entries, n) {
                e.name = format!("name{name}");
                e.notes = notes.map(|n| format!("note{n}"));
                e.folder_id = folder_id;
                e.updated_at = now;
            }
        }
        Op::Rotate { pick: n, pw, dt } => {
            let now = d.tick(dt);
            if let Some(e) = pick(&mut data.entries, n) {
                let password = format!("pw{pw}");
                if e.password != password {
                    e.password_history.push(PastPassword {
                        password: std::mem::replace(&mut e.password, password),
                        replaced_at: now,
                    });
                    if e.password_history.len() > MAX_PASSWORD_HISTORY {
                        e.password_history.remove(0);
                    }
                    e.password_changed_at = Some(now);
                }
                e.updated_at = now;
            }
        }
        Op::Delete { pick: n, dt } => {
            let now = d.tick(dt);
            if let Some(id) = pick(&mut data.entries, n).map(|e| e.id.clone()) {
                data.entries.retain(|e| e.id != id);
                record_deletion(data, &id, now);
            }
        }
        Op::Use { pick: n, dt } => {
            let now = d.tick(dt);
            if let Some(e) = pick(&mut data.entries, n) {
                e.last_used_at = Some(now);
                e.use_count = e.use_count.saturating_add(1);
            }
        }
        Op::AddFolder { name, dt } => {
            let now = d.tick(dt);
            let seq = d.tag().seq;
            let id = format!("f{}-{seq}", d.id);
            data.folders.push(Folder { id, name: format!("folder{name}"), updated_at: now });
        }
        Op::RenameFolder { pick: n, name, dt } => {
            let now = d.tick(dt);
            if let Some(f) = pick(&mut data.folders, n) {
                f.name = format!("folder{name}");
                f.updated_at = now;
            }
        }
        Op::DeleteFolder { pick: n, dt } => {
            let now = d.tick(dt);
            if let Some(id) = pick(&mut data.folders, n).map(|f| f.id.clone()) {
                data.folders.retain(|f| f.id != id);
                record_deletion(data, &id, now);
                for e in data.entries.iter_mut() {
                    if e.folder_id.as_deref() == Some(id.as_str()) {
                        e.folder_id = None;
                    }
                }
            }
        }
    }
}

/// What `sync` compares: contents regardless of order.
fn view(data: &VaultData) -> String {
    String::from_utf8(canonical(data).to_vec()).unwrap()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    #[ignore = "known: findings 1-3 in this file's header; phase 3 fixes them"]
    fn vault_merge_obeys_the_laws(
        base in vec(op(), 0..10),
        a in vec(op(), 0..8),
        b in vec(op(), 0..8),
        c in vec(op(), 0..8),
    ) {
        let (a, b, c) = replicas::<VaultData, _>(&base, &a, &b, &c, apply);
        check_laws(&a, &b, &c, view).map_err(TestCaseError::fail)?;
    }
}

/// The same edits minus deletes, comparing histories regardless of order:
/// isolates finding 3 from findings 1 and 2.
fn op_without_deletes() -> impl Strategy<Value = Op> {
    op().prop_filter("no deletes", |o| !matches!(o, Op::Delete { .. } | Op::DeleteFolder { .. }))
}

proptest! {
    #![proptest_config(config())]

    #[test]
    #[ignore = "known: finding 3 in this file's header; phase 3 fixes it"]
    fn vault_merge_obeys_the_laws_without_deletes(
        base in vec(op_without_deletes(), 0..10),
        a in vec(op_without_deletes(), 0..8),
        b in vec(op_without_deletes(), 0..8),
        c in vec(op_without_deletes(), 0..8),
    ) {
        let (a, b, c) = replicas::<VaultData, _>(&base, &a, &b, &c, apply);
        check_laws(&a, &b, &c, |d| view(&sorted_history(d))).map_err(TestCaseError::fail)?;
    }
}

/// `data` with each history in merge order, to look past ordering.
fn sorted_history(data: &VaultData) -> VaultData {
    let mut d = data.clone();
    for e in d.entries.iter_mut() {
        e.password_history.sort_by(|a, b| (a.replaced_at, &a.password).cmp(&(b.replaced_at, &b.password)));
    }
    d
}
