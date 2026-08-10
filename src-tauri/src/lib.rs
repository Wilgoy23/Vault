// Shared app logic for desktop (main.rs) and mobile (mobile_entry_point).
//
// Desktop-only features — system tray, global shortcut overlay, autostart,
// single-instance, arboard clipboard — are gated behind #[cfg(desktop)].
// Mobile uses the clipboard-manager plugin and the iOS/Android sandbox
// app-data directory for vault storage.

mod crypto;
mod csv_import;
mod vault;

use std::sync::Mutex;
use tauri::{Manager, State};
use vault::{Entry, Folder, VaultData};
use zeroize::{Zeroize, Zeroizing};

#[cfg(desktop)]
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;
#[cfg(desktop)]
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState as SCState,
};

#[cfg(desktop)]
const DEFAULT_SHORTCUT: &str = "Ctrl+Shift+P";
#[cfg(desktop)]
const SHORTCUT_FILE: &str = "shortcut.conf";

struct OverlayShortcut(Mutex<String>);

#[cfg(desktop)]
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

#[cfg(desktop)]
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

#[cfg(desktop)]
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
/// Both fields are None when locked. The key and the vault data both
/// zeroize themselves when dropped/replaced, so locking really does
/// scrub the secrets from memory.
struct AppState {
    key: Option<Zeroizing<[u8; 32]>>,
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

/// Lets the frontend adapt its UI ("windows", "macos", "linux", "ios", "android").
#[tauri::command]
fn get_platform() -> &'static str {
    std::env::consts::OS
}

#[tauri::command]
fn vault_exists() -> bool {
    vault::vault_exists()
}

#[tauri::command]
fn create_vault(mut password: String, state: State<VaultState>) -> Result<(), String> {
    // Immediately unlock after creation; wipe the password either way
    let result = vault::create_vault(&password).and_then(|_| vault::unlock_vault(&password));
    password.zeroize();
    let (key, data) = result?;
    let mut s = state.lock().unwrap();
    s.key = Some(key);
    s.data = Some(data);
    Ok(())
}

#[tauri::command]
fn unlock(mut password: String, state: State<VaultState>) -> Result<(), String> {
    let result = vault::unlock_vault(&password);
    password.zeroize();
    let (key, data) = result?;
    let mut s = state.lock().unwrap();
    s.key = Some(key);
    s.data = Some(data);
    Ok(())
}

#[tauri::command]
fn change_master_password(
    mut current_password: String,
    mut new_password: String,
    state: State<VaultState>,
) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let result = match (&s.key, &s.data) {
        (Some(_), Some(data)) => vault::change_master_password(&current_password, &new_password, data),
        _ => Err("Vault is locked".into()),
    };
    current_password.zeroize();
    new_password.zeroize();
    // Replacing the old key zeroizes it on drop
    s.key = Some(result?);
    Ok(())
}

#[tauri::command]
fn lock(state: State<VaultState>) {
    let mut s = state.lock().unwrap();
    // Dropping these zeroizes the key and all decrypted entries
    s.key = None;
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
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
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
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
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
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_folder(&key, data, name)
}

#[tauri::command]
fn rename_folder(id: String, name: String, state: State<VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::rename_folder(&key, data, &id, name)
}

#[tauri::command]
fn delete_folder(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_folder(&key, data, &id)
}

#[tauri::command]
fn mark_entry_used(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::mark_entry_used(&key, data, &id)
}

#[tauri::command]
fn delete_entry(id: String, state: State<VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_entry(&key, data, &id)
}

// ── Import / Export commands ──────────────────────────────────────────────────
//
// The file dialogs run in Rust rather than the webview, so the webview never
// gets to choose filesystem paths — a compromised frontend can no longer copy
// the vault to (or overwrite it from) an arbitrary location. Both commands
// return false when the user cancels the dialog.
//
// On mobile there is no save dialog: export writes the encrypted backup to
// the app's Documents directory, which is visible in the iOS Files app
// (UIFileSharingEnabled / LSSupportsOpeningDocumentsInPlace in Info.ios.plist).

#[cfg(desktop)]
#[tauri::command]
async fn export_vault(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Vault Backup", &["enc"])
        .set_file_name("vault-backup.enc")
        .blocking_save_file()
    else {
        return Ok(false);
    };
    let dest = picked.into_path().map_err(|e| e.to_string())?;
    vault::export_vault(&dest)?;
    Ok(true)
}

#[cfg(mobile)]
#[tauri::command]
async fn export_vault(app: tauri::AppHandle) -> Result<bool, String> {
    let docs = app
        .path()
        .document_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&docs).map_err(|e| e.to_string())?;
    vault::export_vault(&docs.join("vault-backup.enc"))?;
    Ok(true)
}

#[tauri::command]
async fn import_vault(app: tauri::AppHandle, state: State<'_, VaultState>) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("Vault Backup", &["enc"])
        .blocking_pick_file()
    else {
        return Ok(false);
    };
    let src = picked.into_path().map_err(|e| e.to_string())?;
    vault::import_vault(&src)?;
    // Clear the in-memory session so the user must re-unlock with the new
    // vault's password; dropping the fields zeroizes them
    let mut s = state.lock().unwrap();
    s.key = None;
    s.data = None;
    Ok(true)
}

/// What a CSV import did, reported back to the UI.
#[derive(serde::Serialize)]
struct CsvImportReport {
    imported: usize,
    skipped: usize,
}

/// Imports logins from a browser / password-manager CSV export. The file
/// dialog and the plaintext CSV content stay on the Rust side; the raw
/// content is zeroized after parsing. Returns None when the user cancels.
#[tauri::command]
async fn import_csv(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
) -> Result<Option<CsvImportReport>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app
        .dialog()
        .file()
        .add_filter("CSV Export", &["csv"])
        .blocking_pick_file()
    else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let raw = Zeroizing::new(
        std::fs::read_to_string(&path).map_err(|_| "Cannot read the selected file".to_string())?,
    );
    let parsed = csv_import::parse(&raw)?;

    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    let imported = vault::import_csv_logins(&key, data, parsed.logins)?;
    Ok(Some(CsvImportReport { imported, skipped: parsed.skipped }))
}

// ── Autostart commands (desktop only; mobile stubs return an error) ──────────

#[tauri::command]
fn enable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    return app.autolaunch().enable().map_err(|e| e.to_string());
    #[cfg(mobile)]
    {
        let _ = app;
        Err("Autostart is not available on mobile".into())
    }
}

#[tauri::command]
fn disable_autostart(app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(desktop)]
    return app.autolaunch().disable().map_err(|e| e.to_string());
    #[cfg(mobile)]
    {
        let _ = app;
        Err("Autostart is not available on mobile".into())
    }
}

#[tauri::command]
fn is_autostart_enabled(app: tauri::AppHandle) -> Result<bool, String> {
    #[cfg(desktop)]
    return app.autolaunch().is_enabled().map_err(|e| e.to_string());
    #[cfg(mobile)]
    {
        let _ = app;
        Ok(false)
    }
}

// ── Clipboard ─────────────────────────────────────────────────────────────────

struct ClipboardClearGen(Mutex<u64>);

/// Desktop: writes via arboard so sensitive values can be excluded from
/// Windows' clipboard history / cloud sync. Mobile: uses the
/// clipboard-manager plugin (iOS/Android system pasteboard).
#[tauri::command]
fn write_clipboard_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    #[cfg(desktop)]
    {
        let _ = app;
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;

        #[cfg(target_os = "windows")]
        {
            use arboard::SetExtWindows;
            cb.set().exclude_from_monitoring().text(text).map_err(|e| e.to_string())?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            cb.set_text(text).map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    #[cfg(mobile)]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().write_text(text).map_err(|e| e.to_string())
    }
}

/// Clears the clipboard after `seconds`, unless a newer copy has been made
/// in the meantime. Runs on a background thread so it fires reliably even
/// when the window is hidden/unfocused and JS timers get throttled.
#[tauri::command]
fn schedule_clipboard_clear(app: tauri::AppHandle, state: State<ClipboardClearGen>, seconds: u64) {
    let my_gen = {
        let mut gen = state.0.lock().unwrap();
        *gen += 1;
        *gen
    };

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(seconds));

        let gen_state = app.state::<ClipboardClearGen>();
        if *gen_state.0.lock().unwrap() != my_gen {
            return;
        }

        #[cfg(desktop)]
        if let Ok(mut cb) = arboard::Clipboard::new() {
            let _ = cb.clear();
        }

        #[cfg(mobile)]
        {
            use tauri_plugin_clipboard_manager::ClipboardExt;
            let _ = app.clipboard().write_text(String::new());
        }
    });
}

// ── Overlay shortcut commands (desktop only; mobile stubs) ────────────────────

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
    #[cfg(mobile)]
    {
        let _ = (app, shortcut_str, shortcut_state);
        Err("Global shortcuts are not available on mobile".into())
    }

    #[cfg(desktop)]
    {
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
}

// ── Desktop-only setup (tray, global shortcut, overlay window) ───────────────

#[cfg(desktop)]
fn setup_desktop(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // ── Quick-access overlay window ────────────────────────────────────
    // Created at runtime (not in tauri.conf.json) so mobile never tries
    // to build a second webview window.
    tauri::WebviewWindowBuilder::new(app, "overlay", tauri::WebviewUrl::default())
        .title("Vault — Quick Access")
        .inner_size(480.0, 400.0)
        .decorations(false)
        .always_on_top(true)
        .visible(false)
        .center()
        .skip_taskbar(true)
        .resizable(false)
        .shadow(true)
        .build()?;

    // ── Global hotkey ──────────────────────────────────────────────────
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

    // ── System tray ────────────────────────────────────────────────────
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
                    s.key = None;
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
}

// ── App entry point (called from main.rs on desktop, generated on mobile) ────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    #[cfg(desktop)]
    {
        builder = builder
            // Must be the FIRST plugin registered. When the user launches a second
            // instance (e.g. clicking the pinned taskbar icon again), this callback
            // fires in the already-running instance and the new process exits, so we
            // just restore and focus the existing main window instead of opening a
            // duplicate.
            .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.unminimize();
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }))
            .plugin(tauri_plugin_autostart::init(
                tauri_plugin_autostart::MacosLauncher::LaunchAgent,
                None,
            ))
            .plugin(tauri_plugin_global_shortcut::Builder::new().build());
    }

    #[cfg(mobile)]
    {
        builder = builder.plugin(tauri_plugin_clipboard_manager::init());
    }

    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(AppState::locked()))
        .manage(OverlayShortcut(Mutex::new({
            #[cfg(desktop)]
            { DEFAULT_SHORTCUT.to_string() }
            #[cfg(mobile)]
            { String::new() }
        })))
        .manage(ClipboardClearGen(Mutex::new(0)))
        .invoke_handler(tauri::generate_handler![
            get_platform,
            vault_exists,
            create_vault,
            unlock,
            lock,
            change_master_password,
            is_unlocked,
            list_entries,
            add_entry,
            update_entry,
            delete_entry,
            mark_entry_used,
            list_folders,
            add_folder,
            rename_folder,
            delete_folder,
            export_vault,
            import_vault,
            import_csv,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            get_overlay_shortcut,
            set_overlay_shortcut,
            write_clipboard_text,
            schedule_clipboard_clear,
        ])
        .setup(|app| {
            // On mobile, store the vault inside the app sandbox
            // (iOS: <container>/Library/Application Support). Desktop keeps
            // its historical location so existing vaults still load.
            #[cfg(mobile)]
            vault::set_vault_dir(app.path().app_data_dir()?);

            #[cfg(desktop)]
            setup_desktop(app)?;

            Ok(())
        })
        .on_window_event(|_window, _event| {
            // Hide the windows instead of quitting when closed (desktop only)
            #[cfg(desktop)]
            if let tauri::WindowEvent::CloseRequested { api, .. } = _event {
                if _window.label() == "main" || _window.label() == "overlay" {
                    let _ = _window.hide();
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
