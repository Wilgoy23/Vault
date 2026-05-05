// Prevents a console window appearing on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod vault;

use std::sync::Mutex;
use tauri::State;
use vault::{Entry, VaultData};

/// The session state held in memory while the vault is unlocked.
/// Both fields are None when locked.
struct AppState {
    key: Option<[u8; 32]>,
    data: Option<VaultData>,
}

impl AppState {
    fn locked() -> Self {
        Self { key: None, data: None }
    }
}

type VaultState = Mutex<AppState>;

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
fn vault_exists() -> bool {
    vault::vault_exists()
}

#[tauri::command]
fn create_vault(password: String, state: State<VaultState>) -> Result<(), String> {
    vault::create_vault(&password)?;
    // Immediately unlock after creation
    let (key, data) = vault::unlock_vault(&password)?;
    let mut s = state.lock().unwrap();
    s.key = Some(key);
    s.data = Some(data);
    Ok(())
}

#[tauri::command]
fn unlock(password: String, state: State<VaultState>) -> Result<(), String> {
    let (key, data) = vault::unlock_vault(&password)?;
    let mut s = state.lock().unwrap();
    s.key = Some(key);
    s.data = Some(data);
    Ok(())
}

#[tauri::command]
fn lock(state: State<VaultState>) {
    let mut s = state.lock().unwrap();
    if let Some(mut key) = s.key.take() {
        crypto::wipe_key(&mut key);
    }
    s.data = None;
}

#[tauri::command]
fn is_unlocked(state: State<VaultState>) -> bool {
    state.lock().unwrap().key.is_some()
}

#[tauri::command]
fn list_entries(state: State<VaultState>) -> Result<Vec<Entry>, String> {
    let s = state.lock().unwrap();
    s.data
        .as_ref()
        .map(|d| d.entries.clone())
        .ok_or("Vault is locked".into())
}

#[tauri::command]
fn add_entry(
    name: String,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    state: State<VaultState>,
) -> Result<Entry, String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_entry(&key, data, name, email, password, url, notes)
}

#[tauri::command]
fn update_entry(
    id: String,
    name: String,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    state: State<VaultState>,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::update_entry(&key, data, &id, name, email, password, url, notes)
}

#[tauri::command]
fn delete_entry(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_entry(&key, data, &id)
}

// ── App entry point ───────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .manage(Mutex::new(AppState::locked()))
        .invoke_handler(tauri::generate_handler![
            vault_exists,
            create_vault,
            unlock,
            lock,
            is_unlocked,
            list_entries,
            add_entry,
            update_entry,
            delete_entry,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}