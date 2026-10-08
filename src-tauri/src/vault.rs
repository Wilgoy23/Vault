use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::crypto;

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
const MAX_PASSWORD_HISTORY: usize = 10;

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

/// The format version written by this build. Version 0 is the original
/// headerless format; files without a `version` field are read as 0 and are
/// byte-compatible with 1, so no rewrite is needed to open them.
const CURRENT_VERSION: u32 = 1;

/// On-disk vault file format.
#[derive(Debug, Serialize, Deserialize)]
struct VaultFile {
    #[serde(default)]
    version: u32,
    /// Argon2id cost this file's key was derived with. Missing in v0 files,
    /// which all used the values now in `crypto::DEFAULT_KDF`.
    #[serde(default)]
    kdf: crypto::KdfParams,
    /// Base64-encoded Argon2id salt
    salt: String,
    /// Base64(nonce || AES-GCM ciphertext) of the JSON vault data
    ciphertext: String,
}

impl VaultFile {
    /// A fresh header at the current version and cost.
    fn new(salt: &[u8], ciphertext: String) -> Self {
        VaultFile {
            version: CURRENT_VERSION,
            kdf: crypto::DEFAULT_KDF,
            salt: base64::engine::general_purpose::STANDARD.encode(salt),
            ciphertext,
        }
    }

    fn salt_bytes(&self) -> Result<Vec<u8>, String> {
        base64::engine::general_purpose::STANDARD
            .decode(&self.salt)
            .map_err(|e| e.to_string())
    }
}

/// On mobile, the vault directory is set at startup from Tauri's
/// app-data path (the iOS/Android app sandbox). Desktop keeps the
/// historical `<config_dir>/vault/` location so existing vaults load,
/// unless the user has moved the vault into a sync folder.
static VAULT_DIR: std::sync::RwLock<Option<PathBuf>> = std::sync::RwLock::new(None);

/// Points the vault at `dir`: the app sandbox on mobile, a sync folder on
/// desktop, and a temp directory in the tests.
pub fn set_vault_dir(dir: PathBuf) {
    *VAULT_DIR.write().unwrap() = Some(dir);
}

/// Where the vault lives when it is not in a sync folder.
#[cfg(desktop)]
pub fn default_vault_dir() -> Result<PathBuf, String> {
    let mut dir = dirs::config_dir().ok_or("Could not determine the system config directory")?;
    dir.push("vault");
    Ok(dir)
}

pub fn vault_dir() -> Result<PathBuf, String> {
    if let Some(dir) = VAULT_DIR.read().unwrap().clone() {
        return Ok(dir);
    }
    #[cfg(desktop)]
    {
        default_vault_dir()
    }
    #[cfg(mobile)]
    {
        Err("Vault storage directory has not been initialised".into())
    }
}

fn vault_path() -> Result<PathBuf, String> {
    Ok(vault_dir()?.join("vault.enc"))
}

/// The vault file exactly as this process last read or wrote it. When the
/// file on disk no longer matches, another device (through a sync client)
/// has written to it, and its changes are merged in before ours go out.
/// Ciphertext only, so nothing here needs zeroizing.
static LAST_SEEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

fn remember(raw: &str) {
    *LAST_SEEN.lock().unwrap() = Some(raw.to_owned());
}

fn changed_since_seen(raw: &str) -> bool {
    LAST_SEEN.lock().unwrap().as_deref() != Some(raw)
}

/// What `save_vault` and `sync` report when the file on disk no longer opens
/// with the session key: the master password was changed on another device.
pub const KEY_CHANGED: &str =
    "The master password was changed on another device. Unlock Vault with the new password.";

/// Writes the vault file atomically: write to a temp file in the same
/// directory, fsync, then rename over the target. A crash mid-write can
/// no longer corrupt the only copy of the vault. The file is created
/// owner-readable only (0600) on Unix.
fn write_vault_file(path: &Path, contents: &str) -> Result<(), String> {
    let tmp = path.with_extension("enc.tmp");
    {
        let mut opts = fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
        f.write_all(contents.as_bytes()).map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
    }
    // A sync client uploading the previous version can hold the file open
    // for a moment, which makes the rename fail on Windows. Give it a few tries.
    let mut attempt = 0;
    loop {
        match fs::rename(&tmp, path) {
            Ok(()) => return Ok(()),
            Err(_) if attempt < 4 => {
                attempt += 1;
                std::thread::sleep(std::time::Duration::from_millis(150));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Returns true if a vault file already exists on disk.
pub fn vault_exists() -> bool {
    vault_path().map(|p| p.exists()).unwrap_or(false)
}

/// Creates a new vault with the given master password. Errors if one already exists.
pub fn create_vault(master_password: &str) -> Result<(), String> {
    if vault_exists() {
        return Err("Vault already exists".into());
    }

    let salt = crypto::generate_salt();
    let key = crypto::derive_key(master_password, &salt)?;

    let data = VaultData::default();
    let json = Zeroizing::new(serde_json::to_vec(&data).map_err(|e| e.to_string())?);
    let ciphertext = crypto::encrypt(&json, &key)?;

    let file = VaultFile::new(&salt, ciphertext);

    let path = vault_path()?;
    fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;

    let json_file = serde_json::to_string(&file).map_err(|e| e.to_string())?;
    write_vault_file(&path, &json_file)
}

/// Reads the vault file and checks it is a format this build understands.
/// Returns the raw text alongside the parsed header.
fn read_vault_file() -> Result<(String, VaultFile), String> {
    read_vault_file_at(&vault_path()?)
}

fn read_vault_file_at(path: &Path) -> Result<(String, VaultFile), String> {
    let raw = fs::read_to_string(path)
        .map_err(|_| "Vault file not found".to_string())?;
    let file = parse_vault_file(&raw)?;
    Ok((raw, file))
}

fn parse_vault_file(raw: &str) -> Result<VaultFile, String> {
    let file: VaultFile = serde_json::from_str(raw).map_err(|e| e.to_string())?;

    if file.version > CURRENT_VERSION {
        return Err(format!(
            "This vault was written by a newer version of Vault (format {}). Update Vault to open it.",
            file.version
        ));
    }
    Ok(file)
}

fn parse_vault_data(file: &VaultFile, key: &[u8; 32]) -> Result<VaultData, String> {
    let plaintext = crypto::decrypt(&file.ciphertext, key)?;
    serde_json::from_slice(&plaintext).map_err(|e| format!("Failed to parse vault: {e}"))
}

/// Loads and decrypts the vault. Returns the key (held in app state) and the data.
pub fn unlock_vault(master_password: &str) -> Result<(Zeroizing<[u8; 32]>, VaultData), String> {
    let (raw, file) = read_vault_file()?;

    // Always derive with the parameters the file was written with, not the
    // current defaults, or raising the cost would lock users out.
    let key = crypto::derive_key_with(master_password, &file.salt_bytes()?, file.kdf)?;

    let data = parse_vault_data(&file, &key)?;
    remember(&raw);
    Ok((key, data))
}

/// Opens the vault with a key already in hand, for quick unlock, where the
/// key was kept (wrapped) from an earlier master-password unlock rather than
/// derived again. Fails if the vault file has been replaced since.
pub fn unlock_with_key(key: &[u8; 32]) -> Result<VaultData, String> {
    let (raw, file) = read_vault_file()?;
    let data = parse_vault_data(&file, key)?;
    remember(&raw);
    Ok(data)
}

/// Re-encrypts the vault under a new master password. Verifies the current
/// password against the on-disk file first, then generates a fresh salt and
/// key and rewrites the vault atomically. Returns the new session key.
pub fn change_master_password(
    current_password: &str,
    new_password: &str,
    data: &mut VaultData,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let path = vault_path()?;
    let (raw, file) = read_vault_file_at(&path)?;

    let current_key = crypto::derive_key_with(current_password, &file.salt_bytes()?, file.kdf)?;
    let on_disk = parse_vault_data(&file, &current_key)
        .map_err(|_| "Current password is incorrect".to_string())?;
    // Another device may have written since; those changes must make it into
    // the re-keyed file rather than be overwritten by it.
    if changed_since_seen(&raw) {
        merge(data, on_disk);
    }

    // A new password means a new key, so this is the one moment the vault can
    // move to the current format and cost without knowing the old password twice.
    let new_salt = crypto::generate_salt();
    let new_key = crypto::derive_key(new_password, &new_salt)?;

    let json = Zeroizing::new(serde_json::to_vec(data).map_err(|e| e.to_string())?);
    let ciphertext = crypto::encrypt(&json, &new_key)?;

    let updated = VaultFile::new(&new_salt, ciphertext);

    let out = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    write_vault_file(&path, &out)?;
    remember(&out);
    Ok(new_key)
}

/// Encrypts `data` under `key` and writes it to `path`, keeping the salt and
/// cost from `header`: the key in hand was derived from them, and re-deriving
/// needs the master password. Changing the master password is what moves a
/// vault to the current parameters.
fn write_data(path: &Path, header: &VaultFile, key: &[u8; 32], data: &VaultData) -> Result<(), String> {
    let json = Zeroizing::new(serde_json::to_vec(data).map_err(|e| e.to_string())?);
    let ciphertext = crypto::encrypt(&json, key)?;

    let updated = VaultFile {
        version: header.version.max(CURRENT_VERSION),
        kdf: header.kdf,
        salt: header.salt.clone(),
        ciphertext,
    };

    let out = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    write_vault_file(path, &out)?;
    remember(&out);

    // Every entry and folder change comes through here
    #[cfg(target_os = "ios")]
    crate::autofill::refresh(data);
    Ok(())
}

/// Encrypts and writes VaultData back to disk using the current session key.
/// If another device has written to the file since this one last read it,
/// its changes are merged into `data` first, so neither side's edits are lost.
pub fn save_vault(key: &[u8; 32], data: &mut VaultData) -> Result<(), String> {
    let path = vault_path()?;
    let (raw, file) = read_vault_file_at(&path)?;

    if changed_since_seen(&raw) {
        // Never overwrite a file this key can't read: that is a password
        // change made elsewhere, and writing would undo it.
        let theirs = parse_vault_data(&file, key).map_err(|_| KEY_CHANGED.to_string())?;
        merge(data, theirs);
    }

    write_data(&path, &file, key, data)
}

// ── Sync ──────────────────────────────────────────────────────────────────────
//
// Syncing is left to whatever already syncs a folder (OneDrive, Dropbox,
// iCloud Drive, Syncthing): the vault file simply lives in that folder. Those
// tools only move whole files, so two devices editing offline produce two
// versions of it. Vault reconciles them here, entry by entry, so both
// devices' edits survive. The sync tool only ever sees ciphertext.

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
fn canonical(data: &VaultData) -> Zeroizing<Vec<u8>> {
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

/// The extra copies sync clients leave beside a file two devices changed at
/// once: "vault (Will's conflicted copy).enc" (Dropbox), "vault-LAPTOP.enc"
/// (OneDrive), "vault.sync-conflict-….enc" (Syncthing), "vault 2.enc"
/// (iCloud). Exported backups are named "vault-backup…" and are left alone.
fn conflict_copies(dir: &Path) -> Vec<PathBuf> {
    let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
    read.filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            let Some(name) = p.file_name().and_then(|n| n.to_str()) else { return false };
            let name = name.to_ascii_lowercase();
            p.is_file()
                && name != "vault.enc"
                && name.starts_with("vault")
                && name.ends_with(".enc")
                && !name.starts_with("vault-backup")
        })
        .collect()
}

/// Brings `data` up to date with the vault file, merging in anything another
/// device wrote and any conflict copies a sync client left beside it, then
/// writes the merged vault back if that changed it. Conflict copies are
/// deleted once merged; ones this key can't open are left where they are.
/// Returns true if `data` changed, so the UI knows to reload.
pub fn sync(key: &[u8; 32], data: &mut VaultData) -> Result<bool, String> {
    let path = vault_path()?;
    // Missing for a moment while a sync client swaps the file; try next time
    let Ok((raw, file)) = read_vault_file_at(&path) else { return Ok(false) };

    let remote_changed = changed_since_seen(&raw);
    let conflicts = conflict_copies(path.parent().unwrap_or(Path::new(".")));
    if !remote_changed && conflicts.is_empty() {
        return Ok(false);
    }

    let before = canonical(data);
    // What the file holds now, to decide whether it needs rewriting. When it
    // is unchanged since we wrote it, that is `data` as it stood then.
    let on_disk = if remote_changed {
        let theirs = parse_vault_data(&file, key).map_err(|_| KEY_CHANGED.to_string())?;
        let on_disk = canonical(&theirs);
        merge(data, theirs);
        on_disk
    } else {
        before.clone()
    };

    let mut merged = Vec::new();
    for copy in conflicts {
        let theirs = fs::read_to_string(&copy)
            .ok()
            .and_then(|raw| parse_vault_file(&raw).ok())
            .and_then(|f| parse_vault_data(&f, key).ok());
        if let Some(theirs) = theirs {
            merge(data, theirs);
            merged.push(copy);
        }
    }

    let after = canonical(data);
    if *after != *on_disk || !merged.is_empty() {
        write_data(&path, &file, key, data)?;
    } else {
        // Already in step: just note that this version has been seen
        remember(&raw);
    }
    // Only now that their contents are safely in the main file
    for copy in merged {
        let _ = fs::remove_file(copy);
    }
    Ok(*after != *before)
}

/// Moves the vault file into `dir` (a folder a sync client watches) and
/// points Vault at it. The old local file is kept as `vault.enc.pre-sync`
/// rather than deleted. Refuses if `dir` already holds a vault; that one has
/// to be joined with `join_vault_in` instead.
#[cfg(desktop)]
pub fn move_vault_to(dir: &Path) -> Result<(), String> {
    let src = vault_path()?;
    let dest = dir.join("vault.enc");
    if dest == src {
        return Err("Vault is already in this folder".into());
    }
    if dest.exists() {
        return Err("This folder already contains a vault".into());
    }
    let raw = fs::read_to_string(&src).map_err(|_| "Vault file not found".to_string())?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    write_vault_file(&dest, &raw)?;
    set_vault_dir(dir.to_path_buf());
    let _ = fs::rename(&src, src.with_file_name("vault.enc.pre-sync"));
    Ok(())
}

/// Joins the vault already in `dir`, which another device put there: opens
/// it with `password` (its own master password, which may differ from this
/// vault's), merges this device's `data` into it, and switches to it.
/// Returns that vault's key, which becomes the session key; `data` becomes
/// the merged vault.
#[cfg(desktop)]
pub fn join_vault_in(
    dir: &Path,
    password: &str,
    data: &mut VaultData,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let src = vault_path()?;
    let dest = dir.join("vault.enc");
    let (_, file) = read_vault_file_at(&dest)?;
    let key = crypto::derive_key_with(password, &file.salt_bytes()?, file.kdf)?;
    let theirs = parse_vault_data(&file, &key)
        .map_err(|_| "That password doesn't open the vault in this folder".to_string())?;

    merge(data, theirs);
    write_data(&dest, &file, &key, data)?;
    set_vault_dir(dir.to_path_buf());
    if src != dest {
        let _ = fs::rename(&src, src.with_file_name("vault.enc.pre-sync"));
    }
    Ok(key)
}

/// Points a device with no vault of its own at the one another device keeps
/// in `dir`. The lock screen then asks for that vault's master password.
#[cfg(desktop)]
pub fn open_vault_in(dir: &Path) -> Result<(), String> {
    let dest = dir.join("vault.enc");
    if !dest.exists() {
        return Err("There is no vault.enc in that folder".into());
    }
    read_vault_file_at(&dest)?;
    set_vault_dir(dir.to_path_buf());
    Ok(())
}

/// Copies the vault out of the sync folder back to `home` and points Vault
/// there. The copy in the sync folder is left for the other devices.
#[cfg(desktop)]
pub fn move_vault_home(home: &Path) -> Result<(), String> {
    let src = vault_path()?;
    let dest = home.join("vault.enc");
    if src == dest {
        return Ok(());
    }
    let raw = fs::read_to_string(&src).map_err(|_| "Vault file not found".to_string())?;
    fs::create_dir_all(home).map_err(|e| e.to_string())?;
    write_vault_file(&dest, &raw)?;
    set_vault_dir(home.to_path_buf());
    Ok(())
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Adds a new entry and saves the vault.
pub fn add_entry(
    key: &[u8; 32],
    data: &mut VaultData,
    name: String,
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    folder_id: Option<String>,
    totp_secret: Option<String>,
) -> Result<Entry, String> {
    let now = now_secs();
    let entry = Entry {
        id: Uuid::new_v4().to_string(),
        name,
        kind: EntryKind::Login,
        username,
        email,
        password,
        url,
        notes,
        folder_id,
        totp_secret,
        created_at: now,
        updated_at: now,
        password_changed_at: Some(now),
        password_history: Vec::new(),
        last_used_at: None,
        use_count: 0,
    };
    data.entries.push(entry.clone());
    save_vault(key, data)?;
    Ok(entry)
}

/// Updates an existing entry by id and saves the vault, returning the
/// entry as it now stands.
pub fn update_entry(
    key: &[u8; 32],
    data: &mut VaultData,
    id: &str,
    name: String,
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    folder_id: Option<String>,
    totp_secret: Option<String>,
) -> Result<Entry, String> {
    let entry = data
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or("Entry not found")?;

    let now = now_secs();

    // Only a real password change touches the history and the rotation date,
    // so editing notes or a URL doesn't make the entry look freshly rotated.
    if entry.password != password {
        entry.password_history.push(PastPassword {
            password: std::mem::replace(&mut entry.password, password),
            replaced_at: now,
        });
        if entry.password_history.len() > MAX_PASSWORD_HISTORY {
            entry.password_history.remove(0);
        }
        entry.password_changed_at = Some(now);
    }

    entry.name = name;
    entry.username = username;
    entry.email = email;
    entry.url = url;
    entry.notes = notes;
    entry.folder_id = folder_id;
    entry.totp_secret = totp_secret;
    entry.updated_at = now;

    // Returned so the caller doesn't have to reproduce the history rules
    // above to know what the entry now looks like.
    let updated = entry.clone();
    save_vault(key, data)?;
    Ok(updated)
}

/// Adds a new folder and saves the vault.
pub fn add_folder(key: &[u8; 32], data: &mut VaultData, name: String) -> Result<Folder, String> {
    let folder = Folder { id: Uuid::new_v4().to_string(), name, updated_at: now_secs() };
    data.folders.push(folder.clone());
    save_vault(key, data)?;
    Ok(folder)
}

/// Renames an existing folder by id and saves the vault.
pub fn rename_folder(key: &[u8; 32], data: &mut VaultData, id: &str, name: String) -> Result<(), String> {
    let folder = data.folders.iter_mut().find(|f| f.id == id).ok_or("Folder not found")?;
    folder.name = name;
    folder.updated_at = now_secs();
    save_vault(key, data)
}

/// Deletes a folder by id, unassigns all entries in it, and saves the vault.
pub fn delete_folder(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    let before = data.folders.len();
    data.folders.retain(|f| f.id != id);
    if data.folders.len() == before {
        return Err("Folder not found".into());
    }
    record_deletion(data, id);
    for entry in data.entries.iter_mut() {
        if entry.folder_id.as_deref() == Some(id) {
            entry.folder_id = None;
        }
    }
    save_vault(key, data)
}

/// Records that a credential from the entry was copied, for recency
/// ordering in the overlay, and saves the vault.
pub fn mark_entry_used(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    let entry = data
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or("Entry not found")?;
    entry.last_used_at = Some(now_secs());
    entry.use_count = entry.use_count.saturating_add(1);
    save_vault(key, data)
}

/// Deletes an entry by id and saves the vault.
pub fn delete_entry(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    let before = data.entries.len();
    data.entries.retain(|e| e.id != id);
    if data.entries.len() == before {
        return Err("Entry not found".into());
    }
    record_deletion(data, id);
    save_vault(key, data)
}

/// Leaves a tombstone so a sync merge doesn't restore `id` from a device
/// that hasn't seen the delete yet.
fn record_deletion(data: &mut VaultData, id: &str) {
    data.deleted.retain(|t| t.id != id);
    data.deleted.push(Tombstone { id: id.to_owned(), deleted_at: now_secs() });
}

/// Adds parsed CSV logins to the vault, creating folders as needed, and
/// saves once at the end. Returns the number of entries imported.
pub fn import_csv_logins(
    key: &[u8; 32],
    data: &mut VaultData,
    logins: Vec<crate::csv_import::CsvLogin>,
) -> Result<usize, String> {
    let now = now_secs();
    let count = logins.len();

    for mut login in logins {
        let folder_id = std::mem::take(&mut login.folder).map(|name| {
            match data.folders.iter().find(|f| f.name.eq_ignore_ascii_case(&name)) {
                Some(f) => f.id.clone(),
                None => {
                    let folder = Folder { id: Uuid::new_v4().to_string(), name, updated_at: now };
                    let id = folder.id.clone();
                    data.folders.push(folder);
                    id
                }
            }
        });

        data.entries.push(Entry {
            id: Uuid::new_v4().to_string(),
            name: std::mem::take(&mut login.name),
            kind: EntryKind::Login,
            username: std::mem::take(&mut login.username),
            email: std::mem::take(&mut login.email),
            password: std::mem::take(&mut login.password),
            url: std::mem::take(&mut login.url),
            notes: std::mem::take(&mut login.notes),
            folder_id,
            totp_secret: std::mem::take(&mut login.totp_secret),
            created_at: now,
            updated_at: now,
            password_changed_at: Some(now),
            password_history: Vec::new(),
            last_used_at: None,
            use_count: 0,
        });
    }

    if count > 0 {
        save_vault(key, data)?;
    }
    Ok(count)
}

/// Copies the encrypted vault file to the given destination path.
pub fn export_vault(dest_path: &Path) -> Result<(), String> {
    let src = vault_path()?;
    if !src.exists() {
        return Err("No vault to export".into());
    }
    fs::copy(&src, dest_path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Replaces the current vault file with one from the given source path.
/// Validates that the source is a structurally valid vault file and backs up
/// the existing vault to vault.enc.bak before overwriting, so a bad import
/// (or one whose password the user has forgotten) is recoverable.
pub fn import_vault(src_path: &Path) -> Result<(), String> {
    let raw = fs::read_to_string(src_path)
        .map_err(|_| "Cannot read the selected file".to_string())?;
    let file: VaultFile = serde_json::from_str(&raw)
        .map_err(|_| "Selected file is not a valid vault backup".to_string())?;

    // Shape checks beyond JSON: salt must be a 32-byte base64 value and the
    // ciphertext must at least hold a 12-byte nonce plus a 16-byte GCM tag.
    let salt = base64::engine::general_purpose::STANDARD
        .decode(&file.salt)
        .map_err(|_| "Selected file is not a valid vault backup".to_string())?;
    let ciphertext = base64::engine::general_purpose::STANDARD
        .decode(&file.ciphertext)
        .map_err(|_| "Selected file is not a valid vault backup".to_string())?;
    if salt.len() != 32 || ciphertext.len() < 12 + 16 {
        return Err("Selected file is not a valid vault backup".into());
    }

    let dest = vault_path()?;
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    if dest.exists() {
        let backup = dest.with_extension("enc.bak");
        fs::copy(&dest, &backup)
            .map_err(|e| format!("Could not back up current vault: {e}"))?;
    }
    write_vault_file(&dest, &raw)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    /// `VAULT_DIR` is a process-wide `OnceLock`, so every test in this binary
    /// shares one directory and they must not run concurrently. Each test
    /// takes this guard, which points the vault at a temp directory the first
    /// time and clears any file left by the previous test.
    ///
    /// This matters: without it the real `create_vault`/`save_vault` paths
    /// would write to the developer's actual vault under `<config>/vault/`.
    fn vault_test<'a>() -> MutexGuard<'a, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        static DIR: OnceLock<PathBuf> = OnceLock::new();

        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = DIR.get_or_init(|| {
            let dir = std::env::temp_dir().join("vault_tests");
            fs::create_dir_all(&dir).unwrap();
            dir
        });
        // Every time, since the sync tests move the vault elsewhere
        set_vault_dir(dir.clone());
        assert_eq!(vault_path().unwrap(), dir.join("vault.enc"));
        let _ = fs::remove_file(dir.join("vault.enc"));
        guard
    }

    fn temp_file(name: &str) -> PathBuf {
        std::env::temp_dir().join("vault_tests").join(name)
    }

    fn sample_entry(key: &[u8; 32], data: &mut VaultData, name: &str, password: &str) -> Entry {
        add_entry(
            key,
            data,
            name.into(),
            None,
            "user@example.com".into(),
            password.into(),
            Some("example.com".into()),
            None,
            None,
            None,
        )
        .unwrap()
    }

    #[test]
    fn create_and_unlock_vault() {
        let _g = vault_test();
        create_vault("my-master-password").unwrap();
        let (_key, data) = unlock_vault("my-master-password").unwrap();
        assert!(data.entries.is_empty());
    }

    #[test]
    fn create_refuses_to_overwrite_an_existing_vault() {
        let _g = vault_test();
        create_vault("password").unwrap();
        assert!(create_vault("another-password").is_err());
    }

    #[test]
    fn unlock_fails_with_wrong_password() {
        let _g = vault_test();
        create_vault("correct-password").unwrap();
        assert!(unlock_vault("wrong-password").is_err());
    }

    #[test]
    fn add_entry_persists() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "GitHub", "hunter2");

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert_eq!(data2.entries.len(), 1);
        assert_eq!(data2.entries[0].name, "GitHub");
        assert_eq!(data2.entries[0].email, "user@example.com");
    }

    #[test]
    fn delete_entry_persists() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "To Delete", "pass");

        delete_entry(&key, &mut data, &entry.id).unwrap();
        assert!(delete_entry(&key, &mut data, &entry.id).is_err());

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert!(data2.entries.is_empty());
    }

    #[test]
    fn update_entry_persists() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Old Name", "oldpass");

        update_entry(
            &key, &mut data, &entry.id,
            "New Name".into(), None, "new@example.com".into(), "newpass".into(),
            None, None, None, None,
        ).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert_eq!(data2.entries[0].name, "New Name");
        assert_eq!(data2.entries[0].email, "new@example.com");
        assert_eq!(data2.entries[0].password, "newpass");
    }

    #[test]
    fn changing_the_password_records_history() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Rotating", "first");
        let entry_id = entry.id.clone();

        let mut returned = entry;
        for pw in ["second", "third"] {
            returned = update_entry(
                &key, &mut data, &entry_id,
                "Rotating".into(), None, "user@example.com".into(), pw.into(),
                None, None, None, None,
            ).unwrap();
        }
        // The returned entry reflects the save, so callers need not re-read
        assert_eq!(returned.password, "third");
        assert_eq!(returned.password_history.len(), 2);

        let (_key2, data2) = unlock_vault("password").unwrap();
        let stored = &data2.entries[0];
        assert_eq!(stored.password, "third");
        let history: Vec<&str> = stored
            .password_history
            .iter()
            .map(|p| p.password.as_str())
            .collect();
        assert_eq!(history, vec!["first", "second"]);
    }

    #[test]
    fn editing_other_fields_leaves_the_password_untouched() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Stable", "same-password");
        let changed_at = data.entries[0].password_changed_at;

        update_entry(
            &key, &mut data, &entry.id,
            "Renamed".into(), None, "user@example.com".into(), "same-password".into(),
            None, Some("a note".into()), None, None,
        ).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert!(data2.entries[0].password_history.is_empty());
        assert_eq!(data2.entries[0].password_changed_at, changed_at);
    }

    #[test]
    fn password_history_is_capped() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Busy", "pw-0");

        for n in 1..=(MAX_PASSWORD_HISTORY + 3) {
            update_entry(
                &key, &mut data, &entry.id,
                "Busy".into(), None, "user@example.com".into(), format!("pw-{n}"),
                None, None, None, None,
            ).unwrap();
        }

        let stored = &data.entries[0];
        assert_eq!(stored.password_history.len(), MAX_PASSWORD_HISTORY);
        // Oldest are dropped first, so the window ends just before the current one
        assert_eq!(stored.password, format!("pw-{}", MAX_PASSWORD_HISTORY + 3));
        assert_eq!(stored.password_history[0].password, "pw-3");
    }

    #[test]
    fn folders_round_trip_and_deleting_one_unassigns_its_entries() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();

        let folder = add_folder(&key, &mut data, "Work".into()).unwrap();
        rename_folder(&key, &mut data, &folder.id, "Personal".into()).unwrap();
        let entry = add_entry(
            &key, &mut data, "Filed".into(), None, "a@b.com".into(), "pw".into(),
            None, None, Some(folder.id.clone()), None,
        ).unwrap();
        assert_eq!(entry.folder_id.as_deref(), Some(folder.id.as_str()));

        delete_folder(&key, &mut data, &folder.id).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert!(data2.folders.is_empty());
        assert_eq!(data2.entries[0].folder_id, None);
    }

    #[test]
    fn changing_the_master_password_rekeys_the_vault() {
        let _g = vault_test();
        create_vault("old-password").unwrap();
        let (key, mut data) = unlock_vault("old-password").unwrap();
        sample_entry(&key, &mut data, "Kept", "pw");

        assert!(change_master_password("wrong", "new-password", &mut data).is_err());
        change_master_password("old-password", "new-password", &mut data).unwrap();

        assert!(unlock_vault("old-password").is_err());
        let (_key2, data2) = unlock_vault("new-password").unwrap();
        assert_eq!(data2.entries[0].name, "Kept");
    }

    #[test]
    fn export_then_import_restores_the_vault() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "Backed Up", "pw");

        let backup = temp_file("backup.enc");
        export_vault(&backup).unwrap();

        // Replace it with an unrelated vault, then restore from the backup
        fs::remove_file(vault_path().unwrap()).unwrap();
        create_vault("different-password").unwrap();
        import_vault(&backup).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert_eq!(data2.entries[0].name, "Backed Up");
        assert!(vault_path().unwrap().with_extension("enc.bak").exists());
        let _ = fs::remove_file(&backup);
    }

    #[test]
    fn import_rejects_a_file_that_is_not_a_vault() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let junk = temp_file("junk.enc");
        fs::write(&junk, "not a vault").unwrap();
        assert!(import_vault(&junk).is_err());
        // The existing vault must survive a rejected import
        assert!(unlock_vault("password").is_ok());
        let _ = fs::remove_file(&junk);
    }

    #[test]
    fn a_vault_without_a_header_still_unlocks() {
        let _g = vault_test();
        create_vault("password").unwrap();

        // Rewrite it in the original headerless shape, as an existing
        // installation's file is on disk today
        let path = vault_path().unwrap();
        let file: VaultFile = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        let legacy = serde_json::json!({ "salt": file.salt, "ciphertext": file.ciphertext });
        fs::write(&path, legacy.to_string()).unwrap();

        let (key, mut data) = unlock_vault("password").unwrap();
        assert!(data.entries.is_empty());

        // The next save stamps the current version onto it
        save_vault(&key, &mut data).unwrap();
        let upgraded: VaultFile = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(upgraded.version, CURRENT_VERSION);
        assert_eq!(upgraded.kdf, crypto::DEFAULT_KDF);
    }

    #[test]
    fn a_vault_from_a_newer_format_is_refused() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let path = vault_path().unwrap();
        let mut file: VaultFile =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        file.version = CURRENT_VERSION + 1;
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();

        let err = unlock_vault("password").unwrap_err();
        assert!(err.contains("newer version"), "unexpected error: {err}");
    }

    /// Writes a vault at a deliberately cheap, non-default cost.
    fn write_vault_at_cost(password: &str, kdf: crypto::KdfParams) {
        let path = vault_path().unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let salt = crypto::generate_salt();
        let key = crypto::derive_key_with(password, &salt, kdf).unwrap();
        let json = serde_json::to_vec(&VaultData::default()).unwrap();
        let file = VaultFile {
            version: CURRENT_VERSION,
            kdf,
            salt: base64::engine::general_purpose::STANDARD.encode(salt),
            ciphertext: crypto::encrypt(&json, &key).unwrap(),
        };
        fs::write(&path, serde_json::to_string(&file).unwrap()).unwrap();
    }

    #[test]
    fn unlock_uses_the_cost_stored_in_the_file() {
        let _g = vault_test();
        write_vault_at_cost("password", crypto::KdfParams { m_cost: 8192, t_cost: 1, p_cost: 1 });
        // Deriving at the default cost would produce the wrong key
        unlock_vault("password").unwrap();
    }

    #[test]
    fn changing_the_master_password_moves_the_vault_to_current_parameters() {
        let _g = vault_test();
        write_vault_at_cost("old", crypto::KdfParams { m_cost: 8192, t_cost: 1, p_cost: 1 });

        change_master_password("old", "new", &mut VaultData::default()).unwrap();

        let path = vault_path().unwrap();
        let rekeyed: VaultFile =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(rekeyed.kdf, crypto::DEFAULT_KDF);
        unlock_vault("new").unwrap();
    }

    #[test]
    fn an_entry_without_an_email_round_trips() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        add_entry(
            &key, &mut data, "Router".into(), Some("admin".into()), String::new(),
            "pw".into(), None, None, None, None,
        ).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert_eq!(data2.entries[0].username.as_deref(), Some("admin"));
        assert!(data2.entries[0].email.is_empty());
    }

    #[test]
    fn unlock_with_key_reopens_the_vault_without_the_password() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "Quick", "pw");

        // What quick unlock does: same key, no password, no derivation
        let reopened = unlock_with_key(&key).unwrap();
        assert_eq!(reopened.entries[0].name, "Quick");

        let wrong = crypto::derive_key("other-password", &crypto::generate_salt()).unwrap();
        assert!(unlock_with_key(&wrong).is_err());
    }

    #[test]
    fn unlock_with_key_fails_after_the_vault_is_replaced() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, _data) = unlock_vault("password").unwrap();

        // A backup import swaps in a vault encrypted under a different key
        fs::remove_file(vault_path().unwrap()).unwrap();
        create_vault("a-different-password").unwrap();

        assert!(unlock_with_key(&key).is_err());
    }

    #[test]
    fn mark_entry_used_counts_copies() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Popular", "pw");

        mark_entry_used(&key, &mut data, &entry.id).unwrap();
        mark_entry_used(&key, &mut data, &entry.id).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert_eq!(data2.entries[0].use_count, 2);
        assert!(data2.entries[0].last_used_at.is_some());
    }

    // ── Sync ──────────────────────────────────────────────────────────────

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

    #[test]
    fn sync_merges_another_devices_write_and_a_conflict_copy() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut a) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut a, "From A", "1");
        let a_seen = LAST_SEEN.lock().unwrap().clone();

        // Device B writes the main file through the sync client...
        let (_, mut b) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut b, "From B", "2");

        // ...and device C's offline edit arrives as a conflict copy
        let (_, mut c) = unlock_vault("password").unwrap();
        c.entries.push(entry_at("c", "From C", "3", now_secs()));
        let path = vault_path().unwrap();
        let (_, header) = read_vault_file_at(&path).unwrap();
        let copy = path.with_file_name("vault (C's conflicted copy).enc");
        write_data(&copy, &header, &key, &c).unwrap();
        let backup = path.with_file_name("vault-backup.enc");
        fs::write(&backup, "an export, not a conflict").unwrap();

        // Back on device A
        *LAST_SEEN.lock().unwrap() = a_seen;
        assert!(sync(&key, &mut a).unwrap());

        let mut names: Vec<&str> = a.entries.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["From A", "From B", "From C"]);
        assert!(!copy.exists());
        assert!(backup.exists(), "exports must not be treated as conflict copies");
        assert!(!sync(&key, &mut a).unwrap(), "a second sync has nothing to do");

        let (_, reread) = unlock_vault("password").unwrap();
        assert_eq!(reread.entries.len(), 3);
        let _ = fs::remove_file(&backup);
    }

    #[test]
    fn a_save_never_overwrites_a_password_change_made_elsewhere() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut a) = unlock_vault("password").unwrap();
        let a_seen = LAST_SEEN.lock().unwrap().clone();

        let (_, mut b) = unlock_vault("password").unwrap();
        change_master_password("password", "changed-elsewhere", &mut b).unwrap();

        *LAST_SEEN.lock().unwrap() = a_seen;
        let err = add_entry(
            &key, &mut a, "Late".into(), None, "a@b.com".into(), "pw".into(),
            None, None, None, None,
        ).unwrap_err();
        assert_eq!(err, KEY_CHANGED);
        unlock_vault("changed-elsewhere").unwrap();
    }

    #[test]
    fn joining_a_synced_vault_merges_this_devices_entries_into_it() {
        let _g = vault_test();
        let home = vault_dir().unwrap();
        let synced = home.join("synced");
        let _ = fs::remove_dir_all(&synced);

        // Another device's vault, already in the sync folder
        set_vault_dir(synced.clone());
        create_vault("their-password").unwrap();
        let (their_key, mut theirs) = unlock_vault("their-password").unwrap();
        sample_entry(&their_key, &mut theirs, "Remote", "pw");

        // This device's own vault
        set_vault_dir(home.clone());
        create_vault("my-password").unwrap();
        let (key, mut mine) = unlock_vault("my-password").unwrap();
        sample_entry(&key, &mut mine, "Local", "pw");

        assert!(join_vault_in(&synced, "my-password", &mut mine).is_err());
        join_vault_in(&synced, "their-password", &mut mine).unwrap();

        assert_eq!(vault_path().unwrap(), synced.join("vault.enc"));
        assert!(home.join("vault.enc.pre-sync").exists());
        let (_, joined) = unlock_vault("their-password").unwrap();
        let mut names: Vec<&str> = joined.entries.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["Local", "Remote"]);

        let _ = fs::remove_dir_all(&synced);
        let _ = fs::remove_file(home.join("vault.enc.pre-sync"));
    }

    #[test]
    fn moving_the_vault_into_a_sync_folder_and_back() {
        let _g = vault_test();
        let home = vault_dir().unwrap();
        let synced = home.join("moved");
        let _ = fs::remove_dir_all(&synced);

        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "Travels", "pw");

        move_vault_to(&synced).unwrap();
        assert_eq!(vault_path().unwrap(), synced.join("vault.enc"));
        // The session carries on against the moved file
        sample_entry(&key, &mut data, "Written while synced", "pw");

        move_vault_home(&home).unwrap();
        assert_eq!(vault_path().unwrap(), home.join("vault.enc"));
        let (_, back) = unlock_vault("password").unwrap();
        assert_eq!(back.entries.len(), 2);

        let _ = fs::remove_dir_all(&synced);
        let _ = fs::remove_file(home.join("vault.enc.pre-sync"));
    }
}
