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
/// historical `<config_dir>/vault/` location so existing vaults load.
static VAULT_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

// Called at startup on mobile, and by the tests to keep them off the real vault.
#[cfg_attr(not(any(mobile, test)), allow(dead_code))]
pub fn set_vault_dir(dir: PathBuf) {
    let _ = VAULT_DIR.set(dir);
}

fn vault_path() -> Result<PathBuf, String> {
    if let Some(dir) = VAULT_DIR.get() {
        return Ok(dir.join("vault.enc"));
    }
    #[cfg(desktop)]
    {
        let mut path = dirs::config_dir()
            .ok_or("Could not determine the system config directory")?;
        path.push("vault");
        path.push("vault.enc");
        Ok(path)
    }
    #[cfg(mobile)]
    {
        Err("Vault storage directory has not been initialised".into())
    }
}

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
    fs::rename(&tmp, path).map_err(|e| e.to_string())
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

/// Loads and decrypts the vault. Returns the key (held in app state) and the data.
pub fn unlock_vault(master_password: &str) -> Result<(Zeroizing<[u8; 32]>, VaultData), String> {
    let raw = fs::read_to_string(vault_path()?)
        .map_err(|_| "Vault file not found".to_string())?;

    let file: VaultFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    if file.version > CURRENT_VERSION {
        return Err(format!(
            "This vault was written by a newer version of Vault (format {}). Update Vault to open it.",
            file.version
        ));
    }

    // Always derive with the parameters the file was written with, not the
    // current defaults, or raising the cost would lock users out.
    let key = crypto::derive_key_with(master_password, &file.salt_bytes()?, file.kdf)?;

    let plaintext = crypto::decrypt(&file.ciphertext, &key)?;

    let data: VaultData = serde_json::from_slice(&plaintext)
        .map_err(|e| format!("Failed to parse vault: {e}"))?;

    Ok((key, data))
}

/// Re-encrypts the vault under a new master password. Verifies the current
/// password against the on-disk file first, then generates a fresh salt and
/// key and rewrites the vault atomically. Returns the new session key.
pub fn change_master_password(
    current_password: &str,
    new_password: &str,
    data: &VaultData,
) -> Result<Zeroizing<[u8; 32]>, String> {
    let path = vault_path()?;
    let raw = fs::read_to_string(&path)
        .map_err(|_| "Vault file not found".to_string())?;
    let file: VaultFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let current_key = crypto::derive_key_with(current_password, &file.salt_bytes()?, file.kdf)?;
    crypto::decrypt(&file.ciphertext, &current_key)
        .map_err(|_| "Current password is incorrect".to_string())?;

    // A new password means a new key, so this is the one moment the vault can
    // move to the current format and cost without knowing the old password twice.
    let new_salt = crypto::generate_salt();
    let new_key = crypto::derive_key(new_password, &new_salt)?;

    let json = Zeroizing::new(serde_json::to_vec(data).map_err(|e| e.to_string())?);
    let ciphertext = crypto::encrypt(&json, &new_key)?;

    let updated = VaultFile::new(&new_salt, ciphertext);

    let out = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    write_vault_file(&path, &out)?;
    Ok(new_key)
}

/// Encrypts and writes VaultData back to disk using the current session key.
pub fn save_vault(key: &[u8; 32], data: &VaultData) -> Result<(), String> {
    let path = vault_path()?;
    let raw = fs::read_to_string(&path)
        .map_err(|_| "Vault file not found".to_string())?;
    let file: VaultFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let json = Zeroizing::new(serde_json::to_vec(data).map_err(|e| e.to_string())?);
    let ciphertext = crypto::encrypt(&json, key)?;

    // Salt and cost stay exactly as they were: the session key in hand was
    // derived from them, and re-deriving needs the master password. Changing
    // the master password is what moves a vault to the current parameters.
    let updated = VaultFile {
        version: file.version.max(CURRENT_VERSION),
        kdf: file.kdf,
        salt: file.salt,
        ciphertext,
    };

    let out = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    write_vault_file(&path, &out)
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
    let folder = Folder { id: Uuid::new_v4().to_string(), name };
    data.folders.push(folder.clone());
    save_vault(key, data)?;
    Ok(folder)
}

/// Renames an existing folder by id and saves the vault.
pub fn rename_folder(key: &[u8; 32], data: &mut VaultData, id: &str, name: String) -> Result<(), String> {
    let folder = data.folders.iter_mut().find(|f| f.id == id).ok_or("Folder not found")?;
    folder.name = name;
    save_vault(key, data)
}

/// Deletes a folder by id, unassigns all entries in it, and saves the vault.
pub fn delete_folder(key: &[u8; 32], data: &mut VaultData, id: &str) -> Result<(), String> {
    let before = data.folders.len();
    data.folders.retain(|f| f.id != id);
    if data.folders.len() == before {
        return Err("Folder not found".into());
    }
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
    save_vault(key, data)
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
                    let folder = Folder { id: Uuid::new_v4().to_string(), name };
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
            set_vault_dir(dir.clone());
            dir
        });
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

        assert!(change_master_password("wrong", "new-password", &data).is_err());
        change_master_password("old-password", "new-password", &data).unwrap();

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

        let (key, data) = unlock_vault("password").unwrap();
        assert!(data.entries.is_empty());

        // The next save stamps the current version onto it
        save_vault(&key, &data).unwrap();
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

        change_master_password("old", "new", &VaultData::default()).unwrap();

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
}
