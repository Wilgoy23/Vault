//! Vault's schema 0 and 1 files, read only to convert them to schema 2.
//!
//! Schema 1 stored each entry flat, with one `updated_at` for the whole
//! entry in unix seconds, and merged whole entries. [`migrate`] gives every
//! field the entry's `updated_at` (the password its own
//! `password_changed_at`) as a clock from [`LEGACY_DEVICE`], so a migrated
//! entry merges exactly as before until a device edits it under schema 2.
//! Two devices migrating the same file get identical results.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use super::{EntryView, FolderView, Password, Secret};
use crate::{Counter, Hlc, Immutable, Lww, Max, Unknown, LEGACY_DEVICE};

/// An entry as schema 1 stored it.
pub type Entry = EntryView;

/// A folder as schema 1 stored it.
pub type Folder = FolderView;

/// A deleted entry or folder id. Entries and folders shared one list.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Tombstone {
    pub id: String,
    /// Unix seconds
    pub deleted_at: u64,
}

/// A schema 0 or 1 vault.
#[derive(Debug, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct VaultData {
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    #[serde(default)]
    pub deleted: Vec<Tombstone>,
}

/// Converts a schema 0 or 1 vault to schema 2.
pub fn migrate(old: &VaultData) -> super::VaultData {
    let mut new = super::VaultData::default();

    for (i, f) in old.folders.iter().enumerate() {
        let folder = super::Folder {
            // Schema 1 kept folders in the order they were made
            created: Immutable::new(Hlc::legacy(0, i as u32)),
            name: Lww::new(Secret::new(f.name.clone()), Hlc::legacy(f.updated_at, 0)),
            extra: Unknown::default(),
        };
        new.folders.upsert(f.id.clone(), folder);
    }

    for (i, e) in old.entries.iter().enumerate() {
        let at = Hlc::legacy(e.updated_at, 0);
        let text = |s: &str| Secret::new(s.to_owned());
        let optional = |s: &Option<String>| s.as_deref().map(text);
        let mut uses = Counter::default();
        uses.increment(LEGACY_DEVICE, e.use_count);
        let entry = super::Entry {
            kind: Immutable::new(e.kind),
            created: Immutable::new(Hlc::legacy(e.created_at, i as u32)),
            name: Lww::new(text(&e.name), at),
            username: Lww::new(optional(&e.username), at),
            email: Lww::new(text(&e.email), at),
            password: password(e),
            url: Lww::new(optional(&e.url), at),
            notes: Lww::new(optional(&e.notes), at),
            folder_id: Lww::new(e.folder_id.clone(), at),
            totp_secret: Lww::new(optional(&e.totp_secret), at),
            last_used: e.last_used_at.map(Max::new).unwrap_or_default(),
            uses,
            extra: Unknown::default(),
        };
        new.entries.upsert(e.id.clone(), entry);
    }

    // Schema 1 dropped an entry its tombstone beat, and kept the tombstone
    // beside one edited after the delete. Schema 2's deletes are final, so
    // only ids with nothing left are deleted. Which list an id was in isn't
    // recorded; ids are UUIDs, so it goes in both.
    for t in &old.deleted {
        if new.entries.get(&t.id).is_none() && new.folders.get(&t.id).is_none() {
            let at = Hlc::legacy(t.deleted_at, 0);
            new.entries.delete(t.id.clone(), at);
            new.folders.delete(t.id.clone(), at);
        }
    }

    new
}

/// The password and its history as writes. Schema 1 recorded when each
/// password was replaced, so each is given the time the one before it was
/// replaced (the oldest, the entry's creation), and the current one its
/// change time. The counter keeps them in order when times are equal.
fn password(e: &Entry) -> Password {
    let mut p = Password::default();
    let mut last_ms = 0;
    let mut write = |value: &str, secs: u64, i: usize| {
        last_ms = Hlc::legacy(secs, 0).wall_ms.max(last_ms);
        p.set(Secret::new(value.to_owned()), Hlc::new(last_ms, i as u32, LEGACY_DEVICE));
    };
    let history = &e.password_history;
    for (i, past) in history.iter().enumerate() {
        let written = i.checked_sub(1).map_or(e.created_at, |prev| history[prev].replaced_at);
        write(&past.password, written, i);
    }
    write(&e.password, e.password_changed_at.unwrap_or(e.updated_at), history.len());
    p
}
