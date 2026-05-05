// Prevents a console window appearing on Windows in release builds
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod crypto;
mod vault;

use std::sync::Mutex;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, State,
};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
use vault::{Entry, VaultData};

/// The session state held in memory while the vault is unlocked.
/// Both fields are None when locked.
struct AppState {
    key: Option<[u8; 32]>,
    data: Option<VaultData>,
}

impl AppState {
    fn locked() -> Self {
        Self {
            key: None,
            data: None,
        }
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
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    state: State<VaultState>,
) -> Result<Entry, String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_entry(&key, data, name, username, email, password, url, notes)
}

#[tauri::command]
fn update_entry(
    id: String,
    name: String,
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    state: State<VaultState>,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::update_entry(&key, data, &id, name, username, email, password, url, notes)
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
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
        .setup(|app| {
            // ── Global hotkey ──────────────────────────────────────────────
            let shortcut = Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);

            let app_handle = app.handle().clone();
            app.global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    // Only act on key press, not key release — prevents the toggle
                    // firing twice on Windows (once down, once up)
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let overlay = app_handle.get_webview_window("overlay").unwrap();
                        if overlay.is_visible().unwrap_or(false) {
                            let _ = overlay.hide();
                        } else {
                            let _ = overlay.show();
                            let _ = overlay.set_focus();
                        }
                    }
                })?;

            // ── System tray ────────────────────────────────────────────────
            let open_item = MenuItem::with_id(app, "open", "Open Vault", true, None::<&str>)?;
            let lock_item = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &lock_item, &quit_item])?;

            TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Vault")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "lock" => {
                        if let Some(state) = app.try_state::<VaultState>() {
                            let mut s = state.lock().unwrap();
                            if let Some(mut key) = s.key.take() {
                                crypto::wipe_key(&mut key);
                            }
                            s.data = None;
                        }
                        // Show main window at the lock screen
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // Double-click the tray icon to open the main window
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide the main window instead of quitting when closed
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    window.hide().unwrap();
                    api.prevent_close();
                }
                // Hide overlay on close too
                if window.label() == "overlay" {
                    window.hide().unwrap();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
