//! The merge laws against Vault's records, with each device editing through
//! the same methods Vault's commands call.
//!
//! Phase 2 ran this against schema 1's merge and found three ways it broke
//! the laws. Schema 2 fixes all three:
//!
//! 1. A delete racing edits on two other devices made the result depend on
//!    merge order. Deletes are now final and per record, with no copy dropped
//!    or kept depending on what arrived first.
//! 2. Two password changes in one second reordered history on every merge.
//!    History is now a set of writes ordered by clock.
//! 3. Two devices rotating in the same second broke the tie on password
//!    text and stamped the loser with the winner's time. Hybrid clocks never
//!    tie, and each write keeps its own time.

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
    // Steps of 0 to 2 seconds: same-second edits are common
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

/// The `n`th item this device can see, if any.
fn pick<T>(items: Vec<T>, n: usize) -> Option<T> {
    let len = items.len();
    (len > 0).then(|| items.into_iter().nth(n % len).unwrap())
}

/// The form as it stands for `entry`, as the app fills it in.
fn form(entry: &EntryView) -> Fields {
    Fields {
        name: entry.name.clone(),
        username: entry.username.clone(),
        email: entry.email.clone(),
        password: entry.password.clone(),
        url: entry.url.clone(),
        notes: entry.notes.clone(),
        folder_id: entry.folder_id.clone(),
        totp_secret: entry.totp_secret.clone(),
    }
}

/// One edit, made the way Vault's command for it makes it.
fn apply(data: &mut VaultData, d: &mut Device, op: &Op) {
    let seen = data.latest_clock();
    match *op {
        Op::AddEntry { name, pw, dt } => {
            let now = d.now(dt, seen);
            let seq = d.tag().seq;
            let id = format!("e{}-{seq}", d.id);
            let fields = Fields {
                name: format!("name{name}"),
                username: None,
                email: "user@example.com".into(),
                password: format!("pw{pw}"),
                url: None,
                notes: None,
                folder_id: None,
                totp_secret: None,
            };
            data.add_entry(id, fields, now).unwrap();
        }
        Op::Edit { pick: n, name, notes, folder, dt } => {
            let now = d.now(dt, seen);
            let folder_id = folder.and_then(|f| pick(data.folder_views(), f)).map(|f| f.id.clone());
            if let Some(e) = pick(data.entry_views(), n) {
                let mut f = form(&e);
                f.name = format!("name{name}");
                f.notes = notes.map(|n| format!("note{n}"));
                f.folder_id = folder_id;
                data.update_entry(&e.id, f, now).unwrap();
            }
        }
        Op::Rotate { pick: n, pw, dt } => {
            let now = d.now(dt, seen);
            if let Some(e) = pick(data.entry_views(), n) {
                let mut f = form(&e);
                f.password = format!("pw{pw}");
                data.update_entry(&e.id, f, now).unwrap();
            }
        }
        Op::Delete { pick: n, dt } => {
            let now = d.now(dt, seen);
            if let Some(e) = pick(data.entry_views(), n) {
                data.delete_entry(&e.id, now).unwrap();
            }
        }
        Op::Use { pick: n, dt } => {
            let now = d.now(dt, seen);
            if let Some(e) = pick(data.entry_views(), n) {
                data.mark_entry_used(&e.id, now).unwrap();
            }
        }
        Op::AddFolder { name, dt } => {
            let now = d.now(dt, seen);
            let seq = d.tag().seq;
            let id = format!("f{}-{seq}", d.id);
            data.add_folder(id, format!("folder{name}"), now).unwrap();
        }
        Op::RenameFolder { pick: n, name, dt } => {
            let now = d.now(dt, seen);
            if let Some(f) = pick(data.folder_views(), n) {
                data.rename_folder(&f.id, format!("folder{name}"), now).unwrap();
            }
        }
        Op::DeleteFolder { pick: n, dt } => {
            let now = d.now(dt, seen);
            if let Some(f) = pick(data.folder_views(), n) {
                data.delete_folder(&f.id, now).unwrap();
            }
        }
    }
}

/// The whole state, hidden parts included, as it would be written to disk.
fn view(data: &VaultData) -> String {
    String::from_utf8(canonical(data).to_vec()).unwrap()
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn vault_merge_obeys_the_laws(
        base in vec(op(), 0..10),
        a in vec(op(), 0..8),
        b in vec(op(), 0..8),
        c in vec(op(), 0..8),
    ) {
        let (a, b, c) = replicas::<VaultData, _>(&base, &a, &b, &c, apply);
        check_laws(&a, &b, &c, view).map_err(TestCaseError::fail)?;
    }

    /// What the app shows agrees too, and the file round-trips.
    #[test]
    fn devices_that_merged_the_same_copies_show_the_same_vault(
        base in vec(op(), 0..10),
        a in vec(op(), 0..8),
        b in vec(op(), 0..8),
    ) {
        let (a, b, _) = replicas::<VaultData, _>(&base, &a, &b, &[], apply);
        let ab = folder_crdt::merged(a.clone(), b.clone());
        let ba = folder_crdt::merged(b, a);
        let show = |d: &VaultData| serde_json::to_string(&(d.entry_views(), d.folder_views())).unwrap();
        prop_assert_eq!(show(&ab), show(&ba));

        let reread: VaultData = serde_json::from_slice(&canonical(&ab)).unwrap();
        prop_assert_eq!(reread, ab);
    }
}
