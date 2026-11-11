//! Vault's records, built on the generic types: schema 2.
//!
//! Every field is its own register with its own clock, so two devices that
//! edit different fields of one entry both keep their edit; passwords keep
//! every write, so a rotation that loses still lands in history; use counts
//! add up across devices. Deletes are final ([`DeleteWins`]): a deleted
//! entry's data, passwords included, is dropped from the file on every
//! device.
//!
//! The app never sees these types directly. It edits through the methods on
//! [`VaultData`] and shows [`EntryView`] and [`FolderView`], which keep the
//! flat shape entries had in schema 1, so the UI didn't change. Schema 1
//! files are read and converted by [`legacy`].

pub mod legacy;

use std::fmt;

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{Counter, DeleteWins, Hlc, Immutable, Lww, LwwHistory, Max, Merge, Record, RecordSet, Unknown};

/// The app identifier in Vault's file header.
pub const APP: &str = "vault";

/// The schema this build writes. 0 and 1 are read through [`legacy`].
pub const SCHEMA: u32 = 2;

/// How many superseded passwords each entry keeps.
pub const MAX_PASSWORD_HISTORY: usize = 10;

/// The current password and the ones before it.
pub type Password = LwwHistory<Secret, Hlc, { MAX_PASSWORD_HISTORY + 1 }>;

/// Text wiped from memory when dropped. Every string an entry holds is one,
/// so a value that loses a merge is wiped too, not just left in freed memory.
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(text: String) -> Self {
        Secret(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(..)")
    }
}

/// What a vault item represents. Only `Login` exists today; the field is
/// here so secure notes and cards can be added later without a format
/// change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    #[default]
    Login,
}

// Not secret, but the records zeroize every field and so need this.
impl Zeroize for EntryKind {
    fn zeroize(&mut self) {
        *self = EntryKind::Login;
    }
}

// ── Records ───────────────────────────────────────────────────────────────────

/// A login, as stored and synced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct Entry {
    pub kind: Immutable<EntryKind>,
    /// When the entry was made. Entries are listed in this order.
    pub created: Immutable<Hlc>,
    pub name: Lww<Secret, Hlc>,
    pub username: Lww<Option<Secret>, Hlc>,
    /// May be empty: an entry needs either an email or a username, not both.
    pub email: Lww<Secret, Hlc>,
    pub password: Password,
    pub url: Lww<Option<Secret>, Hlc>,
    pub notes: Lww<Option<Secret>, Hlc>,
    pub folder_id: Lww<Option<String>, Hlc>,
    pub totp_secret: Lww<Option<Secret>, Hlc>,
    /// When a credential was last copied (unix seconds).
    pub last_used: Max<u64>,
    /// How many times a credential has been copied, per device.
    pub uses: Counter,
    #[serde(flatten)]
    pub extra: Unknown,
}

impl Merge for Entry {
    fn merge(&mut self, other: Self) {
        self.kind.merge(other.kind);
        self.created.merge(other.created);
        self.name.merge(other.name);
        self.username.merge(other.username);
        self.email.merge(other.email);
        self.password.merge(other.password);
        self.url.merge(other.url);
        self.notes.merge(other.notes);
        self.folder_id.merge(other.folder_id);
        self.totp_secret.merge(other.totp_secret);
        self.last_used.merge(other.last_used);
        self.uses.merge(other.uses);
        self.extra.merge(other.extra);
    }
}

impl Record for Entry {
    type Clock = Hlc;

    /// The newest edit. Copying a credential doesn't count, as it never has.
    fn last_edit(&self) -> Option<Hlc> {
        [
            self.name.clock(),
            self.username.clock(),
            self.email.clock(),
            self.password.clock(),
            self.url.clock(),
            self.notes.clock(),
            self.folder_id.clock(),
            self.totp_secret.clock(),
        ]
        .into_iter()
        .flatten()
        .max()
        .copied()
    }
}

/// A user-defined folder for grouping entries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize)]
pub struct Folder {
    /// When the folder was made. Folders are listed in this order.
    pub created: Immutable<Hlc>,
    pub name: Lww<Secret, Hlc>,
    #[serde(flatten)]
    pub extra: Unknown,
}

impl Merge for Folder {
    fn merge(&mut self, other: Self) {
        self.created.merge(other.created);
        self.name.merge(other.name);
        self.extra.merge(other.extra);
    }
}

impl Record for Folder {
    type Clock = Hlc;

    fn last_edit(&self) -> Option<Hlc> {
        self.name.clock().copied()
    }
}

/// The decrypted vault contents (serialized to JSON before encryption).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct VaultData {
    pub entries: RecordSet<String, Entry, DeleteWins>,
    pub folders: RecordSet<String, Folder, DeleteWins>,
    #[serde(flatten)]
    pub extra: Unknown,
}

impl Merge for VaultData {
    fn merge(&mut self, mut other: Self) {
        Merge::merge(&mut self.entries, std::mem::take(&mut other.entries));
        Merge::merge(&mut self.folders, std::mem::take(&mut other.folders));
        self.extra.merge(std::mem::take(&mut other.extra));
    }
}

/// `data` serialized, for telling whether a merge changed anything. Every
/// collection is sorted, so equal contents always give equal bytes.
pub fn canonical(data: &VaultData) -> Zeroizing<Vec<u8>> {
    Zeroizing::new(serde_json::to_vec(data).unwrap_or_default())
}

// ── What the app shows ────────────────────────────────────────────────────────

/// A password an entry used to have, kept so a bad rotation can be undone.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct PastPassword {
    pub password: String,
    /// When this password was replaced (unix seconds)
    pub replaced_at: u64,
}

/// An entry as the app shows it: every field's current value. Also the
/// shape entries were stored in up to schema 1, which [`legacy`] reads.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct EntryView {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub kind: EntryKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
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
    /// When the password itself last changed (unix seconds). `None` only on
    /// schema 0 and 1 entries written before this was tracked.
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

/// A folder as the app shows it.
#[derive(Debug, Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct FolderView {
    pub id: String,
    pub name: String,
    /// When the folder was created or last renamed (unix seconds).
    #[serde(default)]
    pub updated_at: u64,
}

/// The fields a person edits, as the app's form sends them.
#[derive(Clone, Default, Zeroize, ZeroizeOnDrop)]
pub struct Fields {
    pub name: String,
    pub username: Option<String>,
    pub email: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub folder_id: Option<String>,
    pub totp_secret: Option<String>,
}

fn text(field: &Lww<Secret, Hlc>) -> String {
    field.get().map_or_else(String::new, |s| s.as_str().to_owned())
}

fn optional(field: &Lww<Option<Secret>, Hlc>) -> Option<String> {
    field.get().and_then(Option::as_ref).map(|s| s.as_str().to_owned())
}

fn secret(text: &mut String) -> Secret {
    Secret(std::mem::take(text))
}

fn optional_secret(text: &mut Option<String>) -> Option<Secret> {
    text.take().map(Secret)
}

/// Writes `value` unless the field already holds it, so saving a form only
/// touches the fields that changed, and an edit elsewhere to another field
/// survives.
fn set_if_changed<T: Ord>(field: &mut Lww<T, Hlc>, value: T, now: Hlc) {
    if field.get() != Some(&value) {
        field.set(value, now);
    }
}

impl Entry {
    /// Every field's current value. A folder that no longer exists reads as
    /// no folder.
    pub fn view(&self, id: &str, folders: &RecordSet<String, Folder, DeleteWins>) -> EntryView {
        // Each superseded password was replaced by the write after it
        let writes: Vec<(&Hlc, &Secret)> = self
            .password
            .clock()
            .zip(self.password.get())
            .into_iter()
            .chain(self.password.history())
            .collect();
        let mut password_history: Vec<PastPassword> = writes
            .windows(2)
            .map(|w| PastPassword { password: w[1].1.as_str().to_owned(), replaced_at: w[0].0.secs() })
            .collect();
        password_history.reverse();

        EntryView {
            id: id.to_owned(),
            name: text(&self.name),
            kind: self.kind.get().copied().unwrap_or_default(),
            username: optional(&self.username),
            email: text(&self.email),
            password: self.password.get().map_or_else(String::new, |s| s.as_str().to_owned()),
            url: optional(&self.url),
            notes: optional(&self.notes),
            folder_id: self.folder_id.get().cloned().flatten().filter(|f| folders.get(f).is_some()),
            totp_secret: optional(&self.totp_secret),
            created_at: self.created.get().map_or(0, Hlc::secs),
            updated_at: self.last_edit().map_or(0, |c| c.secs()),
            password_changed_at: self.password.clock().map(Hlc::secs),
            password_history,
            last_used_at: self.last_used.get().copied(),
            use_count: self.uses.value(),
        }
    }
}

impl Folder {
    pub fn view(&self, id: &str) -> FolderView {
        FolderView {
            id: id.to_owned(),
            name: text(&self.name),
            updated_at: self.last_edit().map_or(0, |c| c.secs()),
        }
    }
}

// ── Edits ─────────────────────────────────────────────────────────────────────
//
// Each of Vault's commands, minus the saving. `now` is a fresh timestamp
// from this device's clock, which must have observed `latest_clock()`.

const NO_ENTRY: &str = "Entry not found";
const NO_FOLDER: &str = "Folder not found";

impl VaultData {
    /// Every entry, oldest first.
    pub fn entry_views(&self) -> Vec<EntryView> {
        let mut entries: Vec<_> = self.entries.iter().collect();
        entries.sort_by(|(a_id, a), (b_id, b)| (a.created.get(), a_id).cmp(&(b.created.get(), b_id)));
        entries.into_iter().map(|(id, e)| e.view(id, &self.folders)).collect()
    }

    pub fn entry_view(&self, id: &str) -> Option<EntryView> {
        self.entries.get(id).map(|e| e.view(id, &self.folders))
    }

    /// Every folder, oldest first.
    pub fn folder_views(&self) -> Vec<FolderView> {
        let mut folders: Vec<_> = self.folders.iter().collect();
        folders.sort_by(|(a_id, a), (b_id, b)| (a.created.get(), a_id).cmp(&(b.created.get(), b_id)));
        folders.into_iter().map(|(id, f)| f.view(id)).collect()
    }

    /// The newest timestamp anywhere in the vault.
    pub fn latest_clock(&self) -> Option<Hlc> {
        self.entries.latest_clock().max(self.folders.latest_clock())
    }

    pub fn add_entry(&mut self, id: String, mut fields: Fields, now: Hlc) -> Result<EntryView, &'static str> {
        if self.entries.is_deleted(&id) || self.entries.get(&id).is_some() {
            return Err("An entry with this id already exists");
        }
        let f = &mut fields;
        let entry = Entry {
            kind: Immutable::new(EntryKind::Login),
            created: Immutable::new(now),
            name: Lww::new(secret(&mut f.name), now),
            username: Lww::new(optional_secret(&mut f.username), now),
            email: Lww::new(secret(&mut f.email), now),
            password: Password::new(secret(&mut f.password), now),
            url: Lww::new(optional_secret(&mut f.url), now),
            notes: Lww::new(optional_secret(&mut f.notes), now),
            folder_id: Lww::new(f.folder_id.take(), now),
            totp_secret: Lww::new(optional_secret(&mut f.totp_secret), now),
            ..Entry::default()
        };
        self.entries.upsert(id.clone(), entry);
        self.entry_view(&id).ok_or(NO_ENTRY)
    }

    /// Saves the form for entry `id`. Only fields whose value changed are
    /// written; a changed password pushes the old one into history.
    pub fn update_entry(&mut self, id: &str, mut fields: Fields, now: Hlc) -> Result<EntryView, &'static str> {
        let e = self.entries.get_mut(id).ok_or(NO_ENTRY)?;
        let f = &mut fields;
        set_if_changed(&mut e.name, secret(&mut f.name), now);
        set_if_changed(&mut e.username, optional_secret(&mut f.username), now);
        set_if_changed(&mut e.email, secret(&mut f.email), now);
        let password = secret(&mut f.password);
        if e.password.get() != Some(&password) {
            e.password.set(password, now);
        }
        set_if_changed(&mut e.url, optional_secret(&mut f.url), now);
        set_if_changed(&mut e.notes, optional_secret(&mut f.notes), now);
        set_if_changed(&mut e.folder_id, f.folder_id.take(), now);
        set_if_changed(&mut e.totp_secret, optional_secret(&mut f.totp_secret), now);
        self.entry_view(id).ok_or(NO_ENTRY)
    }

    /// Records that a credential from entry `id` was copied.
    pub fn mark_entry_used(&mut self, id: &str, now: Hlc) -> Result<(), &'static str> {
        let e = self.entries.get_mut(id).ok_or(NO_ENTRY)?;
        e.uses.increment(now.device, 1);
        e.last_used.set(now.secs());
        Ok(())
    }

    pub fn delete_entry(&mut self, id: &str, now: Hlc) -> Result<(), &'static str> {
        self.entries.get(id).ok_or(NO_ENTRY)?;
        self.entries.delete(id.to_owned(), now);
        Ok(())
    }

    pub fn add_folder(&mut self, id: String, name: String, now: Hlc) -> Result<FolderView, &'static str> {
        if self.folders.is_deleted(&id) || self.folders.get(&id).is_some() {
            return Err("A folder with this id already exists");
        }
        let folder = Folder {
            created: Immutable::new(now),
            name: Lww::new(Secret(name), now),
            extra: Unknown::default(),
        };
        self.folders.upsert(id.clone(), folder);
        self.folders.get(&id).map(|f| f.view(&id)).ok_or(NO_FOLDER)
    }

    pub fn rename_folder(&mut self, id: &str, name: String, now: Hlc) -> Result<(), &'static str> {
        let f = self.folders.get_mut(id).ok_or(NO_FOLDER)?;
        set_if_changed(&mut f.name, Secret(name), now);
        Ok(())
    }

    /// Deletes folder `id` and files its entries under no folder.
    pub fn delete_folder(&mut self, id: &str, now: Hlc) -> Result<(), &'static str> {
        self.folders.get(id).ok_or(NO_FOLDER)?;
        self.folders.delete(id.to_owned(), now);
        for (_, e) in self.entries.iter_mut() {
            if e.folder_id.get().and_then(Option::as_deref) == Some(id) {
                e.folder_id.set(None, now);
            }
        }
        Ok(())
    }
}
