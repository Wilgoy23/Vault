//! Vault's record types and merge rules, moved here unchanged from Vault's
//! `vault.rs`. This module is Vault-specific on purpose: phase 2 replaces it
//! with a generic `RecordSet<T>` and field types, implemented for these
//! structs by hand, then deletes it.
//!
//! Syncing is left to whatever already syncs a folder (OneDrive, Dropbox,
//! iCloud Drive, Syncthing). Those tools only move whole files, so two
//! devices editing offline produce two versions of it. [`merge`] reconciles
//! them entry by entry, so both devices' edits survive.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

/// A user-defined folder for grouping entries.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Folder {
    pub id: String,
    pub name: String,
    /// When the folder was created or last renamed (unix seconds), so a sync
    /// merge can tell which device's name is newer. 0 on folders written
    /// before this field existed.
    #[serde(default)]
    pub updated_at: u64,
}

/// Records that an entry or folder was deleted, so a sync merge doesn't bring
/// it back from another device's copy that still has it. Ids are UUIDs, so
/// entries and folders share one list.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize)]
pub struct Tombstone {
    pub id: String,
    /// Unix seconds
    pub deleted_at: u64,
}

/// What a vault item represents. Only `Login` exists today; the field is
/// here so secure notes and cards can be added later without a format
/// change, and so vaults written now already carry it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Login,
}

// Not secret, but `Entry` zeroizes every field on drop and so needs this.
impl Zeroize for EntryKind {
    fn zeroize(&mut self) {
        *self = EntryKind::Login;
    }
}

/// A password this entry used to have, kept so a bad rotation can be undone.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PastPassword {
    pub password: String,
    /// When this password was replaced (unix seconds)
    pub replaced_at: u64,
}

/// How many superseded passwords to keep per entry. Old ones are dropped
/// oldest-first so the vault cannot grow without bound.
pub const MAX_PASSWORD_HISTORY: usize = 10;

/// A single password entry. Zeroized on drop so plaintext secrets don't
/// linger in freed memory (clones included).
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Entry {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: EntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    /// May be empty: an entry needs either an email or a username, not both.
    #[serde(default)]
    pub email: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub totp_secret: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    /// When the password itself last changed (unix seconds). `None` on
    /// entries written before this field existed; the audit falls back to
    /// `updated_at` for those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password_changed_at: Option<u64>,
    /// Superseded passwords, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub password_history: Vec<PastPassword>,
    /// When a credential from this entry was last copied (unix seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<u64>,
    /// How many times a credential from this entry has been copied.
    #[serde(default)]
    pub use_count: u64,
}

/// The decrypted vault contents (serialized to JSON before encryption).
#[derive(Debug, Default, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct VaultData {
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub folders: Vec<Folder>,
    /// Deleted entry and folder ids. Kept indefinitely: each is a few dozen
    /// bytes, and pruning one would let a long-offline device resurrect it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<Tombstone>,
}

/// Merges another copy of this vault into `ours`. The result is the same
/// whichever side is `ours`, and merging the same copy twice changes nothing,
/// so devices converge however their sync client interleaves the files.
///
/// - Entries and folders: the most recently updated copy wins.
/// - Passwords follow their own clock (`password_changed_at`), so a
///   rotation on one device isn't undone by a notes edit on another. The
///   losing password goes into the history rather than being dropped.
/// - Deletions win over any edit made before them.
pub fn merge(ours: &mut VaultData, mut theirs: VaultData) {
    for tomb in std::mem::take(&mut theirs.deleted) {
        match ours.deleted.iter_mut().find(|t| t.id == tomb.id) {
            Some(t) => t.deleted_at = t.deleted_at.max(tomb.deleted_at),
            None => ours.deleted.push(tomb),
        }
    }

    for entry in std::mem::take(&mut theirs.entries) {
        match ours.entries.iter_mut().find(|e| e.id == entry.id) {
            Some(existing) => merge_entry(existing, entry),
            None => ours.entries.push(entry),
        }
    }

    for folder in std::mem::take(&mut theirs.folders) {
        match ours.folders.iter_mut().find(|f| f.id == folder.id) {
            Some(existing) => {
                if (folder.updated_at, &folder.name) > (existing.updated_at, &existing.name) {
                    *existing = folder;
                }
            }
            None => ours.folders.push(folder),
        }
    }

    let deleted = &ours.deleted;
    let survives = |id: &str, updated_at: u64| {
        deleted.iter().find(|t| t.id == id).map_or(true, |t| updated_at > t.deleted_at)
    };
    ours.entries.retain(|e| survives(&e.id, e.updated_at));
    ours.folders.retain(|f| survives(&f.id, f.updated_at));

    // An entry filed under a folder another device deleted ends up unfiled,
    // as it would have had the delete happened here.
    for entry in ours.entries.iter_mut() {
        if let Some(folder_id) = &entry.folder_id {
            if !ours.folders.iter().any(|f| &f.id == folder_id) {
                entry.folder_id = None;
            }
        }
    }
}

/// A total order on two copies of one entry: newer edit first, then the
/// serialized content, so equal timestamps still pick the same winner on
/// every device instead of each keeping its own.
fn entry_is_newer(a: &Entry, b: &Entry) -> bool {
    if a.updated_at != b.updated_at {
        return a.updated_at > b.updated_at;
    }
    let a_json = Zeroizing::new(serde_json::to_vec(a).unwrap_or_default());
    let b_json = Zeroizing::new(serde_json::to_vec(b).unwrap_or_default());
    *a_json > *b_json
}

fn password_is_newer(a: &Entry, b: &Entry) -> bool {
    (a.password_changed_at.unwrap_or(0), &a.password)
        > (b.password_changed_at.unwrap_or(0), &b.password)
}

fn merge_entry(ours: &mut Entry, mut theirs: Entry) {
    if entry_is_newer(&theirs, ours) {
        std::mem::swap(ours, &mut theirs);
    }
    // `ours` now holds the winning fields and `theirs` the copy it beat,
    // except that the password is decided separately.
    if password_is_newer(&theirs, ours) {
        std::mem::swap(&mut ours.password, &mut theirs.password);
        std::mem::swap(&mut ours.password_changed_at, &mut theirs.password_changed_at);
    }

    ours.last_used_at = ours.last_used_at.max(theirs.last_used_at);
    ours.use_count = ours.use_count.max(theirs.use_count);

    let mut history = std::mem::take(&mut ours.password_history);
    for past in std::mem::take(&mut theirs.password_history) {
        let known = history
            .iter()
            .any(|h| h.password == past.password && h.replaced_at == past.replaced_at);
        if !known {
            history.push(past);
        }
    }
    // Two devices that each rotated the password: keep the one that lost, in
    // case it is the one the website actually has.
    if theirs.password != ours.password && !history.iter().any(|h| h.password == theirs.password) {
        history.push(PastPassword {
            password: std::mem::take(&mut theirs.password),
            replaced_at: ours.password_changed_at.unwrap_or(ours.updated_at),
        });
    }
    history.sort_by(|a, b| (a.replaced_at, &a.password).cmp(&(b.replaced_at, &b.password)));
    if history.len() > MAX_PASSWORD_HISTORY {
        history.drain(..history.len() - MAX_PASSWORD_HISTORY);
    }
    ours.password_history = history;
}

/// `data` in an order-independent form, for telling whether a merge changed
/// anything. Comparing the raw vaults would not do: the same entries in a
/// different order would look like a change and make two devices rewrite the
/// file back and forth.
pub fn canonical(data: &VaultData) -> Zeroizing<Vec<u8>> {
    #[derive(Serialize)]
    struct Canonical<'a> {
        entries: Vec<&'a Entry>,
        folders: Vec<&'a Folder>,
        deleted: Vec<&'a Tombstone>,
    }
    let mut c = Canonical {
        entries: data.entries.iter().collect(),
        folders: data.folders.iter().collect(),
        deleted: data.deleted.iter().collect(),
    };
    c.entries.sort_by(|a, b| a.id.cmp(&b.id));
    c.folders.sort_by(|a, b| a.id.cmp(&b.id));
    c.deleted.sort_by(|a, b| a.id.cmp(&b.id));
    Zeroizing::new(serde_json::to_vec(&c).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry_at(id: &str, name: &str, password: &str, updated_at: u64) -> Entry {
        Entry {
            id: id.into(),
            name: name.into(),
            kind: EntryKind::Login,
            username: None,
            email: "user@example.com".into(),
            password: password.into(),
            url: None,
            notes: None,
            folder_id: None,
            totp_secret: None,
            created_at: 1,
            updated_at,
            password_changed_at: Some(1),
            password_history: Vec::new(),
            last_used_at: None,
            use_count: 0,
        }
    }

    fn vault_of(entries: Vec<Entry>) -> VaultData {
        VaultData { entries, folders: Vec::new(), deleted: Vec::new() }
    }

    fn reparse(data: &VaultData) -> VaultData {
        serde_json::from_slice(&serde_json::to_vec(data).unwrap()).unwrap()
    }

    #[test]
    fn merging_keeps_edits_from_both_sides_whichever_way_round() {
        let a = vault_of(vec![entry_at("1", "Old name", "pw", 10), entry_at("2", "Only on A", "pw", 10)]);
        let b = vault_of(vec![entry_at("1", "New name", "pw", 20), entry_at("3", "Only on B", "pw", 10)]);

        let mut ab = reparse(&a);
        merge(&mut ab, reparse(&b));
        let mut ba = reparse(&b);
        merge(&mut ba, reparse(&a));

        assert_eq!(*canonical(&ab), *canonical(&ba));
        assert_eq!(ab.entries.len(), 3);
        assert_eq!(ab.entries.iter().find(|e| e.id == "1").unwrap().name, "New name");

        // Merging the same copy again changes nothing
        let before = canonical(&ab);
        merge(&mut ab, reparse(&b));
        assert_eq!(*canonical(&ab), *before);
    }

    #[test]
    fn a_password_rotation_survives_a_later_edit_on_another_device() {
        // A rotated the password at t=100; B, not having seen that, edited
        // the notes at t=200
        let mut a = entry_at("1", "Site", "new-pw", 100);
        a.password_changed_at = Some(100);
        a.password_history.push(PastPassword { password: "old-pw".into(), replaced_at: 100 });
        let mut b = entry_at("1", "Site", "old-pw", 200);
        b.notes = Some("edited on B".into());

        let mut merged = vault_of(vec![a]);
        merge(&mut merged, vault_of(vec![b]));
        let e = &merged.entries[0];
        assert_eq!(e.password, "new-pw");
        assert_eq!(e.notes.as_deref(), Some("edited on B"));
        assert_eq!(e.password_history.len(), 1);
    }

    #[test]
    fn two_rotations_keep_the_losing_password_in_history() {
        let mut a = entry_at("1", "Site", "from-a", 100);
        a.password_changed_at = Some(100);
        let mut b = entry_at("1", "Site", "from-b", 150);
        b.password_changed_at = Some(150);

        let mut merged = vault_of(vec![a]);
        merge(&mut merged, vault_of(vec![b]));
        let e = &merged.entries[0];
        assert_eq!(e.password, "from-b");
        assert!(e.password_history.iter().any(|p| p.password == "from-a"));
    }

    #[test]
    fn a_delete_beats_earlier_edits_but_not_later_ones() {
        let mut deleted_here = vault_of(vec![]);
        deleted_here.deleted.push(Tombstone { id: "1".into(), deleted_at: 50 });
        deleted_here.deleted.push(Tombstone { id: "2".into(), deleted_at: 50 });

        let elsewhere = vault_of(vec![entry_at("1", "Stale", "pw", 40), entry_at("2", "Edited after", "pw", 60)]);
        merge(&mut deleted_here, elsewhere);

        let names: Vec<&str> = deleted_here.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["Edited after"]);
    }

    #[test]
    fn a_folder_deleted_elsewhere_unfiles_its_entries() {
        let mut filed = entry_at("1", "Filed", "pw", 10);
        filed.folder_id = Some("f".into());
        let mut here = vault_of(vec![filed]);
        here.folders.push(Folder { id: "f".into(), name: "Work".into(), updated_at: 5 });

        let mut there = vault_of(vec![]);
        there.deleted.push(Tombstone { id: "f".into(), deleted_at: 20 });
        merge(&mut here, there);

        assert!(here.folders.is_empty());
        assert_eq!(here.entries[0].folder_id, None);
    }
}
