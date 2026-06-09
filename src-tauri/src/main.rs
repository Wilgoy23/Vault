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
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState as SCState};
use vault::{Entry, Folder, VaultData};

const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+P";
const SHORTCUT_FILE: &str = "shortcut.conf";

struct OverlayShortcut(Mutex<String>);

fn str_to_code(s: &str) -> Option<Code> {
    match s.to_lowercase().as_str() {
        "a" => Some(Code::KeyA), "b" => Some(Code::KeyB), "c" => Some(Code::KeyC),
        "d" => Some(Code::KeyD), "e" => Some(Code::KeyE), "f" => Some(Code::KeyF),
        "g" => Some(Code::KeyG), "h" => Some(Code::KeyH), "i" => Some(Code::KeyI),
        "j" => Some(Code::KeyJ), "k" => Some(Code::KeyK), "l" => Some(Code::KeyL),
        "m" => Some(Code::KeyM), "n" => Some(Code::KeyN), "o" => Some(Code::KeyO),
        "p" => Some(Code::KeyP), "q" => Some(Code::KeyQ), "r" => Some(Code::KeyR),
        "s" => Some(Code::KeyS), "t" => Some(Code::KeyT), "u" => Some(Code::KeyU),
        "v" => Some(Code::KeyV), "w" => Some(Code::KeyW), "x" => Some(Code::KeyX),
        "y" => Some(Code::KeyY), "z" => Some(Code::KeyZ),
        "0" => Some(Code::Digit0), "1" => Some(Code::Digit1), "2" => Some(Code::Digit2),
        "3" => Some(Code::Digit3), "4" => Some(Code::Digit4), "5" => Some(Code::Digit5),
        "6" => Some(Code::Digit6), "7" => Some(Code::Digit7), "8" => Some(Code::Digit8),
        "9" => Some(Code::Digit9),
        "f1"  => Some(Code::F1),  "f2"  => Some(Code::F2),  "f3"  => Some(Code::F3),
        "f4"  => Some(Code::F4),  "f5"  => Some(Code::F5),  "f6"  => Some(Code::F6),
        "f7"  => Some(Code::F7),  "f8"  => Some(Code::F8),  "f9"  => Some(Code::F9),
        "f10" => Some(Code::F10), "f11" => Some(Code::F11), "f12" => Some(Code::F12),
        "space" => Some(Code::Space), "enter" => Some(Code::Enter),
        "escape" | "esc" => Some(Code::Escape), "tab" => Some(Code::Tab),
        "backspace" => Some(Code::Backspace), "delete" | "del" => Some(Code::Delete),
        "insert" | "ins" => Some(Code::Insert), "home" => Some(Code::Home),
        "end" => Some(Code::End), "pageup" => Some(Code::PageUp),
        "pagedown" => Some(Code::PageDown),
        _ => None,
    }
}

fn parse_shortcut_str(s: &str) -> Option<Shortcut> {
    let parts: Vec<&str> = s.split('+').collect();
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in &parts {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift"            => mods |= Modifiers::SHIFT,
            "alt"              => mods |= Modifiers::ALT,
            "meta" | "win" | "cmd" | "super" => mods |= Modifiers::META,
            key => { code = str_to_code(key); }
        }
    }
    code.map(|c| Shortcut::new(if mods.is_empty() { None } else { Some(mods) }, c))
}

fn toggle_overlay(app: &tauri::AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        if overlay.is_visible().unwrap_or(false) {
            let _ = overlay.hide();
        } else {
            let _ = overlay.show();
            let _ = overlay.set_focus();
        }
    }
}

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
    folder_id: Option<String>,
    totp_secret: Option<String>,
    state: State<VaultState>,
) -> Result<Entry, String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_entry(&key, data, name, username, email, password, url, notes, folder_id, totp_secret)
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
    folder_id: Option<String>,
    totp_secret: Option<String>,
    state: State<VaultState>,
) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::update_entry(&key, data, &id, name, username, email, password, url, notes, folder_id, totp_secret)
}

#[tauri::command]
fn list_folders(state: State<VaultState>) -> Result<Vec<Folder>, String> {
    let s = state.lock().unwrap();
    s.data
        .as_ref()
        .map(|d| d.folders.clone())
        .ok_or("Vault is locked".into())
}

#[tauri::command]
fn add_folder(name: String, state: State<VaultState>) -> Result<Folder, String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_folder(&key, data, name)
}

#[tauri::command]
fn rename_folder(id: String, name: String, state: State<VaultState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::rename_folder(&key, data, &id, name)
}

#[tauri::command]
fn delete_folder(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_folder(&key, data, &id)
}

#[tauri::command]
fn delete_entry(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut s = state.lock().unwrap();
    let key = s.key.ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_entry(&key, data, &id)
}

// ── Import / Export commands ──────────────────────────────────────────────────

#[tauri::command]
fn export_vault(dest_path: String) -> Result<(), String> {
    vault::export_vault(&dest_path)
}

#[tauri::command]
fn import_vault(src_path: String, state: State<VaultState>) -> Result<(), String> {
    vault::import_vault(&src_path)?;
    // Clear the in-memory session so the user must re-unlock with the new vault's password
    let mut s = state.lock().unwrap();
    if let Some(mut key) = s.key.take() {
        crypto::wipe_key(&mut key);
    }
    s.data = None;
    Ok(())
}

// ── Autostart commands ────────────────────────────────────────────────────────

#[tauri::command]
fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    app.autolaunch().enable().map_err(|e| e.to_string())
}

#[tauri::command]
fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    app.autolaunch().disable().map_err(|e| e.to_string())
}

#[tauri::command]
fn is_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

// ── Clipboard ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn clear_clipboard() {
    if let Ok(mut cb) = arboard::Clipboard::new() {
        let _ = cb.clear();
    }
}

// ── Overlay shortcut commands ─────────────────────────────────────────────────

#[tauri::command]
fn get_overlay_shortcut(shortcut_state: State<OverlayShortcut>) -> String {
    shortcut_state.0.lock().unwrap().clone()
}

#[tauri::command]
fn set_overlay_shortcut(
    app: tauri::AppHandle,
    shortcut_str: String,
    shortcut_state: State<OverlayShortcut>,
) -> Result<(), String> {
    let new_sc = parse_shortcut_str(&shortcut_str)
        .ok_or_else(|| format!("Invalid shortcut: {shortcut_str}"))?;

    // Unregister the current shortcut
    let old_str = shortcut_state.0.lock().unwrap().clone();
    if let Some(old_sc) = parse_shortcut_str(&old_str) {
        let _ = app.global_shortcut().unregister(old_sc);
    }

    // Register the new shortcut with the same toggle handler
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(new_sc, move |_app, _sc, event| {
            if event.state() == SCState::Pressed {
                toggle_overlay(&app_handle);
            }
        })
        .map_err(|e| e.to_string())?;

    *shortcut_state.0.lock().unwrap() = shortcut_str.clone();

    // Persist to config file
    if let Ok(data_dir) = app.path().app_data_dir() {
        let _ = std::fs::write(data_dir.join(SHORTCUT_FILE), &shortcut_str);
    }

    Ok(())
}

// ── App entry point ───────────────────────────────────────────────────────────

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(Mutex::new(AppState::locked()))
        .manage(OverlayShortcut(Mutex::new(DEFAULT_SHORTCUT.to_string())))
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
            list_folders,
            add_folder,
            rename_folder,
            delete_folder,
            export_vault,
            import_vault,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            get_overlay_shortcut,
            set_overlay_shortcut,
            clear_clipboard,
        ])
        .setup(|app| {
            // ── Global hotkey ──────────────────────────────────────────────
            // Load persisted shortcut or fall back to default
            let shortcut_str = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|d| std::fs::read_to_string(d.join(SHORTCUT_FILE)).ok())
                .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string());

            // Update managed state to match what we'll actually register
            *app.state::<OverlayShortcut>().0.lock().unwrap() = shortcut_str.clone();

            let shortcut = parse_shortcut_str(&shortcut_str)
                .unwrap_or_else(|| Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP));

            let app_handle = app.handle().clone();
            app.global_shortcut()
                .on_shortcut(shortcut, move |_app, _shortcut, event| {
                    // Only act on key press, not key release — prevents the toggle
                    // firing twice on Windows (once down, once up)
                    if event.state() == SCState::Pressed {
                        toggle_overlay(&app_handle);
                    }
                })?;

            // ── System tray ────────────────────────────────────────────────
            let open_item = MenuItem::with_id(app, "open", "Open Vault", true, None::<&str>)?;
            let lock_item = MenuItem::with_id(app, "lock", "Lock", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_item, &lock_item, &quit_item])?;

            let tray_icon = tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray-icon.png"
            ))?;

            TrayIconBuilder::new()
                .icon(tray_icon)
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
