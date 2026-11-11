use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde_json::{Map, Value};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::crypto::{self, KdfParams};

// The records, their edits and their merge live in the folder-crdt crate,
// and so does reading and writing the file and keeping it in step with
// other devices. The app shows entries and folders in their flat form,
// re-exported here under the names the rest of the app uses.
pub use folder_crdt::vault::{EntryView as Entry, FolderView as Folder, VaultData};
use folder_crdt::codec::{read_params, Envelope};
use folder_crdt::format::FormatError;
use folder_crdt::vault::file::{self as vault_file, FileCodec};
use folder_crdt::vault::Fields;
use folder_crdt::{Cipher, FolderStore, Hlc, HlcClock, KeyMismatch, Merge, Store, StoreError};

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

/// This device's clock. Every edit is stamped from it, and it observes every
/// vault loaded or merged, so an edit always sorts after what it was made on.
static CLOCK: Mutex<Option<HlcClock>> = Mutex::new(None);

/// Loads this device's id from `dir`, creating it on first run. `dir` must
/// be the app's local data folder, never the sync folder: the id tells
/// devices apart. Without it (or in tests) each run gets a fresh random id,
/// which is still correct, just less tidy.
pub fn set_device_dir(dir: &Path) {
    match folder_crdt::fs::device_id(dir) {
        Ok(id) => *CLOCK.lock().unwrap() = Some(HlcClock::new(id)),
        Err(e) => eprintln!("Could not store the device id, using a temporary one: {e}"),
    }
}

fn with_clock<R>(f: impl FnOnce(&mut HlcClock) -> R) -> R {
    let mut clock = CLOCK.lock().unwrap();
    let clock = clock.get_or_insert_with(|| {
        HlcClock::new(folder_crdt::fs::random_device_id().unwrap_or(u64::MAX))
    });
    f(clock)
}

/// A fresh timestamp for an edit.
fn now() -> Hlc {
    with_clock(HlcClock::now)
}

/// Moves this device's clock past everything in `data`.
fn observe(data: &VaultData) {
    if let Some(latest) = data.latest_clock() {
        with_clock(|c| c.observe(latest));
    }
}

// ── The file ──────────────────────────────────────────────────────────────────

/// Vault's encryption, handed to folder-crdt: AES-256-GCM under the key
/// derived from the master password. The Argon2id salt and cost it was
/// derived with are stored in cleartext beside the ciphertext, so the key
/// can be derived again.
pub struct VaultCipher {
    key: Zeroizing<[u8; 32]>,
    kdf: KdfParams,
    salt: Vec<u8>,
}

impl VaultCipher {
    fn new(key: &[u8; 32], kdf: KdfParams, salt: Vec<u8>) -> Self {
        VaultCipher { key: Zeroizing::new(*key), kdf, salt }
    }

    /// A new salt and key for `password`, at the current cost.
    fn fresh(password: &str) -> Result<Self, String> {
        let salt = crypto::generate_salt();
        let key = crypto::derive_key(password, &salt)?;
        Ok(VaultCipher::new(&key, crypto::DEFAULT_KDF, salt.to_vec()))
    }

    /// The key for the file `bytes`, derived from `password` with the salt
    /// and cost the file was written with, not the current defaults, or
    /// raising the cost would lock users out.
    fn for_file(password: &str, bytes: &[u8]) -> Result<Self, String> {
        let (kdf, salt) = file_params(bytes)?;
        let key = crypto::derive_key_with(password, &salt, kdf)?;
        Ok(VaultCipher::new(&key, kdf, salt))
    }
}

impl Cipher for VaultCipher {
    fn params(&self) -> Map<String, Value> {
        let mut params = Map::new();
        params.insert("kdf".into(), serde_json::to_value(self.kdf).unwrap_or_default());
        params.insert("salt".into(), B64.encode(&self.salt).into());
        params
    }

    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
        crypto::seal(plaintext, &self.key).map_err(|e| StoreError::Io(io::Error::other(e)))
    }

    fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyMismatch> {
        crypto::open(sealed, &self.key).map_err(|_| KeyMismatch)
    }
}

/// The Argon2id cost and salt in a vault file's cleartext. Files from before
/// the cost was stored all used the values now in `crypto::DEFAULT_KDF`.
fn file_params(bytes: &[u8]) -> Result<(KdfParams, Vec<u8>), String> {
    let params = read_params(bytes).map_err(describe)?;
    let kdf = match params.get("kdf") {
        Some(kdf) => serde_json::from_value(kdf.clone()).map_err(|e| format!("Failed to parse vault: {e}"))?,
        None => crypto::DEFAULT_KDF,
    };
    let salt = params
        .get("salt")
        .and_then(Value::as_str)
        .and_then(|s| B64.decode(s).ok())
        .ok_or("Failed to parse vault: no salt")?;
    Ok((kdf, salt))
}

/// Reads a vault file and checks it is a layout this build can open.
fn read_vault_file(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|_| "Vault file not found".to_string())?;
    vault_file::check(&bytes).map_err(describe)?;
    Ok(bytes)
}

/// What `save_vault` and `sync` report when the file on disk no longer opens
/// with the session key: the master password was changed on another device.
pub const KEY_CHANGED: &str =
    "The master password was changed on another device. Unlock Vault with the new password.";

/// The message the user sees for a store error.
fn describe(e: StoreError) -> String {
    match e {
        StoreError::KeyMismatch => KEY_CHANGED.into(),
        StoreError::Format(FormatError::Newer { schema, .. }) => format!(
            "This vault was written by a newer version of Vault (format {schema}). Update Vault to open it."
        ),
        StoreError::Format(e) => format!("This is not a Vault file: {e}"),
        StoreError::Corrupt(why) => format!("Failed to parse vault: {why}"),
        StoreError::Io(e) if e.kind() == io::ErrorKind::NotFound => "Vault file not found".into(),
        StoreError::Io(e) => e.to_string(),
    }
}

/// Like `describe`, for opening a vault, where a key that doesn't fit means
/// a wrong password.
fn describe_unlock(e: StoreError) -> String {
    match e {
        StoreError::KeyMismatch => crypto::WRONG_KEY.into(),
        e => describe(e),
    }
}

type VaultStore = FolderStore<FileCodec<VaultCipher>>;

/// The vault file, with the bytes this process last read or wrote of it.
/// When the file no longer matches them, another device (through a sync
/// client) has written to it, and its changes are merged in before ours go
/// out. Replaced on every unlock.
static STORE: Mutex<Option<VaultStore>> = Mutex::new(None);

/// Opens the vault in the vault folder with `cipher`, and makes it the
/// store later saves go through.
fn open_store(cipher: VaultCipher) -> Result<VaultData, String> {
    let mut store = vault_file::store(vault_dir()?, cipher);
    let data = store.load().map_err(describe_unlock)?;
    observe(&data);
    *STORE.lock().unwrap() = Some(store);
    Ok(data)
}

/// Runs `f` on the store for the vault folder, under `key`. Rebuilt when the
/// folder or key changed since the last unlock: the vault was moved, or a
/// test is playing another device.
fn with_store<R>(key: &[u8; 32], f: impl FnOnce(&mut VaultStore) -> Result<R, StoreError>) -> Result<R, String> {
    let dir = vault_dir()?;
    let mut guard = STORE.lock().unwrap();
    let current = guard
        .as_ref()
        .is_some_and(|s| s.dir() == dir && *s.codec().cipher().key == *key);
    if !current {
        // This key was derived with the salt and cost in the file
        let (kdf, salt) = file_params(&read_vault_file(&dir.join("vault.enc"))?)?;
        let codec = FileCodec::new(VaultCipher::new(key, kdf, salt));
        match guard.as_mut() {
            Some(store) if store.dir() == dir => store.set_codec(codec),
            _ => *guard = Some(FolderStore::new(dir, "vault", "enc", codec)),
        }
        // A key that differs from the last one must open the file before
        // anything is written with it
        guard.as_mut().unwrap().forget();
    }
    f(guard.as_mut().unwrap()).map_err(describe)
}

/// Writes `contents` atomically (temp file, fsync, rename), so a crash
/// mid-write can never corrupt the only copy of the vault.
fn write_vault_file(path: &Path, contents: &[u8]) -> Result<(), String> {
    folder_crdt::fs::write_atomic(path, contents).map_err(|e| e.to_string())
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
    let mut store = vault_file::store(vault_dir()?, VaultCipher::fresh(master_password)?);
    store.create(&VaultData::default()).map_err(describe)
}

/// Loads and decrypts the vault. Returns the key (held in app state) and the
/// data. A vault from before schema 2 is converted, and written in the
/// current layout with the next save.
pub fn unlock_vault(master_password: &str) -> Result<(Zeroizing<[u8; 32]>, VaultData), String> {
    let bytes = read_vault_file(&vault_path()?)?;
    let cipher = VaultCipher::for_file(master_password, &bytes)?;
    let key = cipher.key.clone();
    Ok((key, open_store(cipher)?))
}

/// Opens the vault with a key already in hand, for quick unlock, where the
/// key was kept (wrapped) from an earlier master-password unlock rather than
/// derived again. Fails if the vault file has been replaced since.
pub fn unlock_with_key(key: &[u8; 32]) -> Result<VaultData, String> {
    let (kdf, salt) = file_params(&read_vault_file(&vault_path()?)?)?;
    open_store(VaultCipher::new(key, kdf, salt))
}

/// Re-encrypts the vault under a new master password. Verifies the current
/// password against the on-disk file first, merging in anything another
/// device wrote, then generates a fresh salt and key and rewrites the vault
/// atomically. Returns the new session key.
pub fn change_master_password(
    current_password: &str,
    new_password: &str,
    data: &mut VaultData,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let bytes = read_vault_file(&vault_path()?)?;
    let mut store = vault_file::store(vault_dir()?, VaultCipher::for_file(current_password, &bytes)?);
    let on_disk = store.load().map_err(|_| "Current password is incorrect".to_string())?;
    data.merge(on_disk);
    observe(data);

    // A new password means a new key, so this is the one moment the vault can
    // move to the current cost without knowing the old password twice.
    let cipher = VaultCipher::fresh(new_password)?;
    let key = cipher.key.clone();
    store.set_codec(FileCodec::new(cipher));
    store.save(data).map_err(describe)?;
    *STORE.lock().unwrap() = Some(store);
    Ok(key)
}

/// Encrypts and writes VaultData back to disk using the current session key.
/// If another device has written to the file since this one last read it,
/// its changes are merged into `data` first, so neither side's edits are
/// lost. A file this key can't open is never overwritten: that is a password
/// change made elsewhere, and writing would undo it.
pub fn save_vault(key: &[u8; 32], data: &mut VaultData) -> Result<(), String> {
    with_store(key, |store| store.save(data))?;
    observe(data);

    // Every entry and folder change comes through here
    #[cfg(target_os = "ios")]
    crate::autofill::refresh(data);
    Ok(())
}

// ── Sync ──────────────────────────────────────────────────────────────────────
//
// Syncing is left to whatever already syncs a folder (OneDrive, Dropbox,
// iCloud Drive, Syncthing): the vault file simply lives in that folder. Those
// tools only move whole files, so two devices editing offline produce two
// versions of it. folder-crdt's FolderStore reconciles them, field by field,
// so both devices' edits survive. The sync tool only ever sees ciphertext.

/// Brings `data` up to date with the vault file, merging in anything another
/// device wrote and any conflict copies a sync client left beside it, then
/// writes the merged vault back if that changed it. Conflict copies are
/// deleted once merged; ones this key can't open are left where they are.
/// Returns true if `data` changed, so the UI knows to reload.
pub fn sync(key: &[u8; 32], data: &mut VaultData) -> Result<bool, String> {
    // Missing for a moment while a sync client swaps the file; try next time
    if !vault_exists() {
        return Ok(false);
    }
    let outcome = with_store(key, |store| store.sync(data))?;
    observe(data);
    for (copy, e) in &outcome.unreadable {
        eprintln!("Left conflict copy {} unmerged: {e}", copy.display());
    }
    #[cfg(target_os = "ios")]
    if outcome.wrote {
        crate::autofill::refresh(data);
    }
    Ok(outcome.changed)
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
    let raw = fs::read(&src).map_err(|_| "Vault file not found".to_string())?;
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
    let bytes = read_vault_file(&dest)?;
    let cipher = VaultCipher::for_file(password, &bytes)?;
    let key = cipher.key.clone();
    let mut store = vault_file::store(dir, cipher);
    let theirs = store.load().map_err(|e| match e {
        StoreError::KeyMismatch => "That password doesn't open the vault in this folder".to_string(),
        e => describe(e),
    })?;

    data.merge(theirs);
    observe(data);
    store.save(data).map_err(describe)?;
    *STORE.lock().unwrap() = Some(store);
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
    read_vault_file(&dest)?;
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
    let raw = fs::read(&src).map_err(|_| "Vault file not found".to_string())?;
    fs::create_dir_all(home).map_err(|e| e.to_string())?;
    write_vault_file(&dest, &raw)?;
    set_vault_dir(home.to_path_buf());
    Ok(())
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
    let fields = Fields { name, username, email, password, url, notes, folder_id, totp_secret };
    let id = Uuid::new_v4().to_string();
    data.add_entry(id.clone(), fields, now())?;
    save_vault(key, data)?;
    data.entry_view(&id).ok_or_else(|| "Entry not found".into())
}

/// Saves the edit form for an entry and returns the entry as it now
/// stands. Only the fields that changed are written, so an edit made on
/// another device to a different field survives; a changed password moves
/// the old one into history.
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
    let fields = Fields { name, username, email, password, url, notes, folder_id, totp_secret };
    data.update_entry(id, fields, now())?;
    save_vault(key, data)?;
    // After the save, which may have merged in another device's edits
    data.entry_view(id).ok_or_else(|| "Entry not found".into())
}

/// Adds a new folder and saves the vault.
pub fn add_folder(key: &[u8; 32], data: &mut VaultData, name: String) -> Result<Folder, String> {
    let folder = data.add_folder(Uuid::new_v4().to_string(), name, now())?;
    save_vault(key, data)?;
    Ok(folder)
}

/// Renames an existing folder by id and saves the vault.
pub fn rename_folder(key: &[u8; 32], data: &mut VaultData, id: &str, name: String) -> Result<(), String> {
    data.rename_folder(id, name, now())?;
    save_vault(key, data)
}

/// Deletes a folder by id, unassigns all entries in it, and saves the vault.
pub fn delete_folder(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    data.delete_folder(id, now())?;
    save_vault(key, data)
}

/// Records that a credential from the entry was copied, for recency
/// ordering in the overlay, and saves the vault.
pub fn mark_entry_used(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    data.mark_entry_used(id, now())?;
    save_vault(key, data)
}

/// Deletes an entry by id and saves the vault. The delete is final: the
/// entry and its passwords leave the file on every device.
pub fn delete_entry(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    data.delete_entry(id, now())?;
    save_vault(key, data)
}

/// Adds parsed CSV logins to the vault, creating folders as needed, and
/// saves once at the end. Returns the number of entries imported.
pub fn import_csv_logins(
    key: &[u8; 32],
    data: &mut VaultData,
    logins: Vec<crate::csv_import::CsvLogin>,
) -> Result<usize, String> {
    let count = logins.len();

    for mut login in logins {
        let folder_id = match std::mem::take(&mut login.folder) {
            None => None,
            Some(name) => {
                let existing = data.folder_views().into_iter().find(|f| f.name.eq_ignore_ascii_case(&name));
                match existing {
                    Some(f) => Some(f.id.clone()),
                    None => Some(data.add_folder(Uuid::new_v4().to_string(), name, now())?.id.clone()),
                }
            }
        };

        let fields = Fields {
            name: std::mem::take(&mut login.name),
            username: std::mem::take(&mut login.username),
            email: std::mem::take(&mut login.email),
            password: std::mem::take(&mut login.password),
            url: std::mem::take(&mut login.url),
            notes: std::mem::take(&mut login.notes),
            folder_id,
            totp_secret: std::mem::take(&mut login.totp_secret),
        };
        data.add_entry(Uuid::new_v4().to_string(), fields, now())?;
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
    let raw = fs::read(src_path).map_err(|_| "Cannot read the selected file".to_string())?;
    let invalid = || "Selected file is not a valid vault backup".to_string();
    let envelope = Envelope::parse(&raw).map_err(|_| invalid())?;

    // Shape checks beyond JSON: salt must be a 32-byte base64 value and the
    // ciphertext must at least hold a 12-byte nonce plus a 16-byte GCM tag.
    let salt = envelope
        .params
        .get("salt")
        .and_then(Value::as_str)
        .and_then(|s| B64.decode(s).ok())
        .ok_or_else(invalid)?;
    if salt.len() != 32 || envelope.payload.len() < 12 + 16 {
        return Err(invalid());
    }
    // A backup from a newer Vault would only fail at the next unlock
    vault_file::check(&raw).map_err(describe)?;

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
    use folder_crdt::format::Header;
    use folder_crdt::vault::{APP, MAX_PASSWORD_HISTORY, SCHEMA};
    use folder_crdt::Codec;
    use serde::{Deserialize, Serialize};

    const CURRENT_VERSION: u32 = SCHEMA;

    /// The vault file's layout, to inspect and forge files.
    #[derive(Debug, Serialize, Deserialize)]
    struct VaultFile {
        #[serde(default)]
        version: u32,
        #[serde(flatten, default)]
        header: Option<Header>,
        #[serde(default)]
        kdf: KdfParams,
        salt: String,
        ciphertext: String,
    }

    fn read_file() -> VaultFile {
        serde_json::from_slice(&fs::read(vault_path().unwrap()).unwrap()).unwrap()
    }

    /// Writes `data` at `path` as this device would, under `key`.
    fn write_copy(path: &Path, key: &[u8; 32], data: &VaultData) {
        let (kdf, salt) = file_params(&fs::read(vault_path().unwrap()).unwrap()).unwrap();
        let bytes = FileCodec::new(VaultCipher::new(key, kdf, salt)).encode(data).unwrap();
        fs::write(path, bytes).unwrap();
    }
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

    /// A login's form, for editing `VaultData` directly as another device.
    fn fields(name: &str, password: &str) -> Fields {
        Fields {
            name: name.into(),
            username: None,
            email: "user@example.com".into(),
            password: password.into(),
            url: None,
            notes: None,
            folder_id: None,
            totp_secret: None,
        }
    }

    #[test]
    fn create_and_unlock_vault() {
        let _g = vault_test();
        create_vault("my-master-password").unwrap();
        let (_key, data) = unlock_vault("my-master-password").unwrap();
        assert!(data.entry_views().is_empty());
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
        assert_eq!(data2.entry_views().len(), 1);
        assert_eq!(data2.entry_views()[0].name, "GitHub");
        assert_eq!(data2.entry_views()[0].email, "user@example.com");
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
        assert!(data2.entry_views().is_empty());
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
        assert_eq!(data2.entry_views()[0].name, "New Name");
        assert_eq!(data2.entry_views()[0].email, "new@example.com");
        assert_eq!(data2.entry_views()[0].password, "newpass");
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
        let stored = data2.entry_views().remove(0);
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
        let changed_at = data.entry_views()[0].password_changed_at;

        update_entry(
            &key, &mut data, &entry.id,
            "Renamed".into(), None, "user@example.com".into(), "same-password".into(),
            None, Some("a note".into()), None, None,
        ).unwrap();

        let (_key2, data2) = unlock_vault("password").unwrap();
        assert!(data2.entry_views()[0].password_history.is_empty());
        assert_eq!(data2.entry_views()[0].password_changed_at, changed_at);
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

        let stored = data.entry_views().remove(0);
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
        assert!(data2.folder_views().is_empty());
        assert_eq!(data2.entry_views()[0].folder_id, None);
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
        assert_eq!(data2.entry_views()[0].name, "Kept");
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
        assert_eq!(data2.entry_views()[0].name, "Backed Up");
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

    // ── Versions ──────────────────────────────────────────────────────────

    /// A schema 1 vault's contents, as installations hold them today.
    fn schema_1_contents() -> serde_json::Value {
        serde_json::json!({
            "entries": [{
                "id": "bank", "name": "Bank", "email": "me@example.com",
                "password": "pw2", "url": null, "notes": null,
                "created_at": 100, "updated_at": 300, "password_changed_at": 300,
                "password_history": [{ "password": "pw1", "replaced_at": 300 }],
                "use_count": 4
            }],
            "folders": [{ "id": "f", "name": "Money", "updated_at": 100 }],
            "deleted": [{ "id": "gone", "deleted_at": 200 }]
        })
    }

    /// Writes `contents` as the vault, encrypted under `password`, in the
    /// file layout of `version` (`None`: the original headerless one).
    fn write_old_vault(password: &str, version: Option<u32>, contents: &serde_json::Value) {
        let path = vault_path().unwrap();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let salt = crypto::generate_salt();
        let key = crypto::derive_key(password, &salt).unwrap();
        let mut file = serde_json::json!({
            "salt": base64::engine::general_purpose::STANDARD.encode(salt),
            "ciphertext": crypto::encrypt(contents.to_string().as_bytes(), &key).unwrap(),
        });
        if let Some(version) = version {
            file["version"] = version.into();
            file["kdf"] = serde_json::to_value(crypto::DEFAULT_KDF).unwrap();
        }
        fs::write(&path, file.to_string()).unwrap();
    }

    /// What the vault file holds, decrypted.
    fn contents_on_disk(key: &[u8; 32]) -> (VaultFile, serde_json::Value) {
        let file = read_file();
        let plaintext = crypto::decrypt(&file.ciphertext, key).unwrap();
        let contents = serde_json::from_slice(&plaintext).unwrap();
        (file, contents)
    }

    #[test]
    fn a_vault_without_a_header_unlocks_and_is_upgraded_on_save() {
        let _g = vault_test();
        write_old_vault("password", None, &serde_json::json!({
            "entries": [{
                "id": "1", "name": "Old", "password": "pw", "url": null, "notes": null,
                "created_at": 5, "updated_at": 6
            }]
        }));

        let (key, mut data) = unlock_vault("password").unwrap();
        assert_eq!(data.entry_views()[0].name, "Old");

        // The next save writes it in the current format
        save_vault(&key, &mut data).unwrap();
        let (upgraded, _) = contents_on_disk(&key);
        assert_eq!(upgraded.version, CURRENT_VERSION);
        assert_eq!(upgraded.header, Some(Header::new(APP, SCHEMA)));
        assert_eq!(upgraded.kdf, crypto::DEFAULT_KDF);
        assert_eq!(unlock_vault("password").unwrap().1.entry_views()[0].name, "Old");
    }

    #[test]
    fn a_schema_1_vault_keeps_everything_through_the_upgrade() {
        let _g = vault_test();
        write_old_vault("password", Some(1), &schema_1_contents());

        let (key, mut data) = unlock_vault("password").unwrap();
        // An edit saves it as schema 2
        mark_entry_used(&key, &mut data, "bank").unwrap();

        let (_, reread) = unlock_vault("password").unwrap();
        let e = reread.entry_views().remove(0);
        assert_eq!((e.name.as_str(), e.password.as_str()), ("Bank", "pw2"));
        assert_eq!(e.password_history[0].password, "pw1");
        assert_eq!(e.password_history[0].replaced_at, 300);
        assert_eq!(e.use_count, 5);
        assert_eq!(reread.folder_views()[0].name, "Money");
        assert!(reread.entries.is_deleted("gone"));
    }

    #[test]
    fn a_schema_1_conflict_copy_from_a_device_not_yet_updated_merges_in() {
        let _g = vault_test();
        write_old_vault("password", Some(1), &schema_1_contents());
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "Added after the upgrade", "pw");

        // A device still on schema 1 changed the bank password offline
        let mut theirs = schema_1_contents();
        theirs["entries"][0]["password"] = "pw3".into();
        theirs["entries"][0]["updated_at"] = 400.into();
        theirs["entries"][0]["password_changed_at"] = 400.into();
        theirs["entries"][0]["password_history"] = serde_json::json!([
            { "password": "pw1", "replaced_at": 300 },
            { "password": "pw2", "replaced_at": 400 }
        ]);
        let header = read_file();
        let copy = serde_json::json!({
            "version": 1,
            "kdf": header.kdf,
            "salt": header.salt,
            "ciphertext": crypto::encrypt(theirs.to_string().as_bytes(), &key).unwrap(),
        });
        let copy_path = vault_path().unwrap().with_file_name("vault (old laptop's conflicted copy).enc");
        fs::write(&copy_path, copy.to_string()).unwrap();

        assert!(sync(&key, &mut data).unwrap());
        assert!(!copy_path.exists());
        let bank = data.entry_view("bank").unwrap();
        assert_eq!(bank.password, "pw3");
        let history: Vec<&str> = bank.password_history.iter().map(|p| p.password.as_str()).collect();
        assert_eq!(history, ["pw1", "pw2"]);
        assert_eq!(data.entry_views().len(), 2);
    }

    #[test]
    fn a_vault_from_a_newer_format_is_refused() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let path = vault_path().unwrap();
        let original: serde_json::Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();

        // By the version builds before schema 2 read, or by the header
        for (field, value) in [("version", CURRENT_VERSION + 1), ("schema", SCHEMA + 1), ("format", 99)] {
            let mut file = original.clone();
            file[field] = value.into();
            fs::write(&path, file.to_string()).unwrap();
            let err = unlock_vault("password").unwrap_err();
            assert!(err.contains("newer version"), "{field}: unexpected error: {err}");
        }

        let mut other_app = original.clone();
        other_app["app"] = "chronicle".into();
        fs::write(&path, other_app.to_string()).unwrap();
        assert!(unlock_vault("password").unwrap_err().contains("not a Vault file"));
    }

    #[test]
    fn fields_from_a_newer_build_survive_a_save_from_this_one() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        let entry = sample_entry(&key, &mut data, "Site", "pw");

        // A newer build of Vault adds fields to the vault and to the entry
        let (file, mut contents) = contents_on_disk(&key);
        contents["breach_check"] = serde_json::json!({ "last_run": 123 });
        contents["entries"]["records"][&entry.id]["passkey"] = serde_json::json!([[1, 0, 9], "key"]);
        let rewritten = VaultFile {
            ciphertext: crypto::encrypt(contents.to_string().as_bytes(), &key).unwrap(),
            ..file
        };
        fs::write(vault_path().unwrap(), serde_json::to_string(&rewritten).unwrap()).unwrap();

        // This build opens it, edits and saves
        let (key, mut data) = unlock_vault("password").unwrap();
        mark_entry_used(&key, &mut data, &entry.id).unwrap();

        let (_, saved) = contents_on_disk(&key);
        assert_eq!(saved["breach_check"], serde_json::json!({ "last_run": 123 }));
        assert_eq!(saved["entries"]["records"][&entry.id]["passkey"], serde_json::json!([[1, 0, 9], "key"]));
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
            header: Some(Header::new(APP, SCHEMA)),
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
        assert_eq!(data2.entry_views()[0].username.as_deref(), Some("admin"));
        assert!(data2.entry_views()[0].email.is_empty());
    }

    #[test]
    fn unlock_with_key_reopens_the_vault_without_the_password() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut data) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut data, "Quick", "pw");

        // What quick unlock does: same key, no password, no derivation
        let reopened = unlock_with_key(&key).unwrap();
        assert_eq!(reopened.entry_views()[0].name, "Quick");

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
        assert_eq!(data2.entry_views()[0].use_count, 2);
        assert!(data2.entry_views()[0].last_used_at.is_some());
    }

    // ── Sync ──────────────────────────────────────────────────────────────

    #[test]
    fn sync_merges_another_devices_write_and_a_conflict_copy() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut a) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut a, "From A", "1");
        let a_seen = STORE.lock().unwrap().take();

        // Device B writes the main file through the sync client...
        let (_, mut b) = unlock_vault("password").unwrap();
        sample_entry(&key, &mut b, "From B", "2");

        // ...and device C's offline edit arrives as a conflict copy
        let (_, mut c) = unlock_vault("password").unwrap();
        c.add_entry("c".into(), fields("From C", "3"), now()).unwrap();
        let path = vault_path().unwrap();
        let copy = path.with_file_name("vault (C's conflicted copy).enc");
        write_copy(&copy, &key, &c);
        let backup = path.with_file_name("vault-backup.enc");
        fs::write(&backup, "an export, not a conflict").unwrap();

        // Back on device A
        *STORE.lock().unwrap() = a_seen;
        assert!(sync(&key, &mut a).unwrap());

        let mut names: Vec<String> = a.entry_views().iter().map(|e| e.name.clone()).collect();
        names.sort();
        assert_eq!(names, vec!["From A", "From B", "From C"]);
        assert!(!copy.exists());
        assert!(backup.exists(), "exports must not be treated as conflict copies");
        assert!(!sync(&key, &mut a).unwrap(), "a second sync has nothing to do");

        let (_, reread) = unlock_vault("password").unwrap();
        assert_eq!(reread.entry_views().len(), 3);
        let _ = fs::remove_file(&backup);
    }

    #[test]
    fn a_save_never_overwrites_a_password_change_made_elsewhere() {
        let _g = vault_test();
        create_vault("password").unwrap();
        let (key, mut a) = unlock_vault("password").unwrap();
        let a_seen = STORE.lock().unwrap().take();

        let (_, mut b) = unlock_vault("password").unwrap();
        change_master_password("password", "changed-elsewhere", &mut b).unwrap();

        *STORE.lock().unwrap() = a_seen;
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
        let mut names: Vec<String> = joined.entry_views().iter().map(|e| e.name.clone()).collect();
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
        assert_eq!(back.entry_views().len(), 2);

        let _ = fs::remove_dir_all(&synced);
        let _ = fs::remove_file(home.join("vault.enc.pre-sync"));
    }
}
