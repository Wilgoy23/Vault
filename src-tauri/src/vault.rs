use base64::Engine;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

use crate::crypto;

/// A single password entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub email: String,
    pub password: String,
    pub url: Option<String>,
    pub notes: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// The decrypted vault contents (serialized to JSON before encryption).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VaultData {
    pub entries: Vec<Entry>,
}

/// On-disk vault file format.
#[derive(Debug, Serialize, Deserialize)]
struct VaultFile {
    /// Base64-encoded Argon2id salt
    salt: String,
    /// Base64(nonce || AES-GCM ciphertext) of the JSON vault data
    ciphertext: String,
}

fn vault_path() -> PathBuf {
    let mut path = dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    path.push("vault");
    path.push("vault.enc");
    path
}

/// Returns true if a vault file already exists on disk.
pub fn vault_exists() -> bool {
    vault_path().exists()
}

/// Creates a new vault with the given master password. Errors if one already exists.
pub fn create_vault(master_password: &str) -> Result<(), String> {
    if vault_exists() {
        return Err("Vault already exists".into());
    }

    let salt = crypto::generate_salt();
    let mut key = crypto::derive_key(master_password, &salt)?;

    let data = VaultData::default();
    let json = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
    let ciphertext = crypto::encrypt(&json, &key)?;

    crypto::wipe_key(&mut key);

    let file = VaultFile {
        salt: base64::engine::general_purpose::STANDARD.encode(salt),
        ciphertext,
    };

    let vault_dir = vault_path().parent().unwrap().to_path_buf();
    fs::create_dir_all(&vault_dir).map_err(|e| e.to_string())?;

    let json_file = serde_json::to_string(&file).map_err(|e| e.to_string())?;
    fs::write(vault_path(), json_file).map_err(|e| e.to_string())?;

    Ok(())
}

/// Loads and decrypts the vault. Returns the key (held in app state) and the data.
pub fn unlock_vault(master_password: &str) -> Result<([u8; 32], VaultData), String> {
    let raw = fs::read_to_string(vault_path())
        .map_err(|_| "Vault file not found".to_string())?;

    let file: VaultFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let salt_bytes = base64::engine::general_purpose::STANDARD
        .decode(&file.salt)
        .map_err(|e| e.to_string())?;

    let key = crypto::derive_key(master_password, &salt_bytes)?;

    let plaintext = crypto::decrypt(&file.ciphertext, &key)?;

    let data: VaultData = serde_json::from_slice(&plaintext)
        .map_err(|e| format!("Failed to parse vault: {e}"))?;

    Ok((key, data))
}

/// Encrypts and writes VaultData back to disk using the current session key.
pub fn save_vault(key: &[u8; 32], data: &VaultData) -> Result<(), String> {
    let raw = fs::read_to_string(vault_path())
        .map_err(|_| "Vault file not found".to_string())?;
    let file: VaultFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;

    let json = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    let ciphertext = crypto::encrypt(&json, key)?;

    let updated = VaultFile {
        salt: file.salt,
        ciphertext,
    };

    let out = serde_json::to_string(&updated).map_err(|e| e.to_string())?;
    fs::write(vault_path(), out).map_err(|e| e.to_string())?;

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
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
) -> Result<Entry, String> {
    let now = now_secs();
    let entry = Entry {
        id: Uuid::new_v4().to_string(),
        name,
        email,
        password,
        url,
        notes,
        created_at: now,
        updated_at: now,
    };
    data.entries.push(entry.clone());
    save_vault(key, data)?;
    Ok(entry)
}

/// Updates an existing entry by id and saves the vault.
pub fn update_entry(
    key: &[u8; 32],
    data: &mut VaultData,
    id: &str,
    name: String,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    let entry = data
        .entries
        .iter_mut()
        .find(|e| e.id == id)
        .ok_or("Entry not found")?;

    entry.name = name;
    entry.email = email;
    entry.password = password;
    entry.url = url;
    entry.notes = notes;
    entry.updated_at = now_secs();

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