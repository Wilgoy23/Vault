// Shared app logic for desktop (main.rs) and mobile (mobile_entry_point).
//
// Desktop-only features — system tray, global shortcut overlay, autostart,
// single-instance, arboard clipboard — are gated behind #[cfg(desktop)].
// Mobile uses the clipboard-manager plugin and the iOS/Android sandbox
// app-data directory for vault storage.

#[cfg(target_os = "ios")]
mod autofill;
mod crypto;
mod csv_import;
#[cfg(all(desktop, target_os = "windows"))]
mod hello;
mod vault;

use std::sync::Mutex;
use tauri::{Emitter, Manager, State};
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
        "arrowup" | "up" => Some(Code::ArrowUp), "arrowdown" | "down" => Some(Code::ArrowDown),
        "arrowleft" | "left" => Some(Code::ArrowLeft), "arrowright" | "right" => Some(Code::ArrowRight),
        "minus" => Some(Code::Minus), "equal" => Some(Code::Equal),
        "comma" => Some(Code::Comma), "period" => Some(Code::Period),
        "slash" => Some(Code::Slash), "backslash" => Some(Code::Backslash),
        "semicolon" => Some(Code::Semicolon), "quote" => Some(Code::Quote),
        "backquote" => Some(Code::Backquote),
        "bracketleft" => Some(Code::BracketLeft), "bracketright" => Some(Code::BracketRight),
        _ => None,
    }
}

/// Function keys are the only keys safe to bind without a modifier; a bare
/// letter/digit/punctuation hotkey would swallow that key system-wide.
#[cfg(desktop)]
fn is_function_key(code: Code) -> bool {
    matches!(
        code,
        Code::F1 | Code::F2 | Code::F3 | Code::F4 | Code::F5 | Code::F6
            | Code::F7 | Code::F8 | Code::F9 | Code::F10 | Code::F11 | Code::F12
    )
}

/// Parses "Ctrl+Shift+P" style strings. Returns an error message describing
/// why a string was rejected so the UI can show it.
#[cfg(desktop)]
fn parse_shortcut_str(s: &str) -> Result<Shortcut, String> {
    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;
    for part in s.split('+') {
        match part.trim().to_lowercase().as_str() {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "shift"            => mods |= Modifiers::SHIFT,
            "alt"              => mods |= Modifiers::ALT,
            "meta" | "win" | "cmd" | "super" => mods |= Modifiers::META,
            key => {
                code = Some(str_to_code(key).ok_or_else(|| format!("Unsupported key: {part}"))?);
            }
        }
    }
    let code = code.ok_or_else(|| "Shortcut needs a key".to_string())?;
    // Shift alone still leaves the key typeable in most apps, so require a
    // "real" modifier for anything that isn't a function key.
    let has_real_modifier = mods.intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::META);
    if !has_real_modifier && !is_function_key(code) {
        return Err("Shortcut needs Ctrl, Alt or Win/Cmd (or use an F-key)".into());
    }
    Ok(Shortcut::new(if mods.is_empty() { None } else { Some(mods) }, code))
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

// Anything below that derives a key (Argon2id at 64 MiB) or writes the vault
// file is an `async fn`: Tauri runs sync commands on the main thread, where a
// few hundred milliseconds of hashing or an fsync freezes both windows and the
// tray. There is nothing to await inside them; `async` alone moves them to the
// thread pool.

#[tauri::command]
async fn create_vault(mut password: String, state: State<'_, VaultState>) -> Result<(), String> {
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
async fn unlock(mut password: String, state: State<'_, VaultState>) -> Result<(), String> {
    let result = vault::unlock_vault(&password);
    password.zeroize();
    let (key, data) = result?;
    // Catches up on changes AutoFill can't have seen, such as a backup import
    #[cfg(target_os = "ios")]
    autofill::refresh(&data);
    let mut s = state.lock().unwrap();
    s.key = Some(key);
    s.data = Some(data);
    Ok(())
}

#[tauri::command]
async fn change_master_password(
    mut current_password: String,
    mut new_password: String,
    state: State<'_, VaultState>,
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

/// Clears the session and tells every window so none keeps showing (or
/// holding in JS memory) entries that the backend has already scrubbed.
fn lock_and_notify(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<VaultState>() {
        let mut s = state.lock().unwrap();
        // Dropping these zeroizes the key and all decrypted entries
        s.key = None;
        s.data = None;
    }
    let _ = app.emit("vault-locked", ());
}

// ── Windows Hello quick unlock ────────────────────────────────────────────────
//
// See hello.rs for the design. The vault key is wrapped with a key only the
// TPM can reproduce, and the wrapped copy lives here in memory for the life of
// the process. Nothing is written to disk, so a cold start always needs the
// master password.

/// The vault key, encrypted under a key that Hello alone can reproduce.
#[cfg(all(desktop, target_os = "windows"))]
struct ArmedQuickUnlock {
    /// Signed by the Hello credential to regenerate the wrapping key
    challenge: [u8; 32],
    /// base64(nonce || AES-GCM ciphertext) of the 32-byte vault key
    wrapped_key: String,
}

#[cfg(all(desktop, target_os = "windows"))]
struct QuickUnlock(Mutex<Option<ArmedQuickUnlock>>);

/// Whether this machine can do Hello at all. The UI hides the option when not.
#[tauri::command]
fn quick_unlock_available() -> bool {
    #[cfg(all(desktop, target_os = "windows"))]
    return hello::is_available();
    #[cfg(not(all(desktop, target_os = "windows")))]
    false
}

/// Whether a key is currently wrapped and waiting, so the lock screen knows
/// whether to offer the Hello button.
#[tauri::command]
fn quick_unlock_armed(_app: tauri::AppHandle) -> bool {
    #[cfg(all(desktop, target_os = "windows"))]
    return _app
        .try_state::<QuickUnlock>()
        .map(|s| s.0.lock().unwrap().is_some())
        .unwrap_or(false);
    #[cfg(not(all(desktop, target_os = "windows")))]
    false
}

/// Wraps the current vault key so Hello can reopen it later. Prompts for the
/// gesture once, here, while the user is present — not at lock time, when
/// they have walked away.
#[tauri::command]
async fn arm_quick_unlock(_app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(all(desktop, target_os = "windows"))]
    {
        let state = _app.try_state::<VaultState>().ok_or("Vault is locked")?;
        let key = {
            let s = state.lock().unwrap();
            let key = s.key.as_deref().ok_or("Vault is locked")?;
            Zeroizing::new(*key)
        };

        let mut challenge = [0u8; 32];
        {
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut challenge);
        }

        let wrapping_key = hello::derive_wrapping_key(&challenge)?;
        let wrapped_key = crypto::encrypt(&key[..], &wrapping_key)?;

        let armed = _app.try_state::<QuickUnlock>().ok_or("Quick unlock unavailable")?;
        *armed.0.lock().unwrap() = Some(ArmedQuickUnlock { challenge, wrapped_key });
        Ok(())
    }
    #[cfg(not(all(desktop, target_os = "windows")))]
    Err("Windows Hello is not available on this platform".into())
}

/// Reopens the vault with the wrapped key, after a Hello gesture. Returns an
/// error the lock screen can show; the master password always still works.
#[tauri::command]
async fn quick_unlock(_app: tauri::AppHandle) -> Result<(), String> {
    #[cfg(all(desktop, target_os = "windows"))]
    {
        let armed = _app.try_state::<QuickUnlock>().ok_or("Quick unlock unavailable")?;
        let (challenge, wrapped_key) = {
            let guard = armed.0.lock().unwrap();
            let a = guard.as_ref().ok_or("Quick unlock is not set up")?;
            (a.challenge, a.wrapped_key.clone())
        };

        let wrapping_key = hello::derive_wrapping_key(&challenge)?;
        let unwrapped = crypto::decrypt(&wrapped_key, &wrapping_key)?;
        if unwrapped.len() != 32 {
            return Err("Quick unlock data is corrupt".into());
        }
        let mut key = Zeroizing::new([0u8; 32]);
        key.copy_from_slice(&unwrapped);

        // The vault file may have been replaced (a backup import) since the
        // key was wrapped, in which case this key no longer opens it.
        let data = vault::unlock_with_key(&key).map_err(|_| {
            "The vault has changed since quick unlock was set up. Use your master password."
                .to_string()
        })?;

        let state = _app.try_state::<VaultState>().ok_or("Vault is unavailable")?;
        let mut s = state.lock().unwrap();
        s.key = Some(key);
        s.data = Some(data);
        drop(s);

        let _ = _app.emit("vault-unlocked", ());
        Ok(())
    }
    #[cfg(not(all(desktop, target_os = "windows")))]
    Err("Windows Hello is not available on this platform".into())
}

/// Drops the wrapped key and deletes Vault's Hello credential, for when the
/// user turns the feature off.
#[tauri::command]
fn disarm_quick_unlock(_app: tauri::AppHandle) {
    #[cfg(all(desktop, target_os = "windows"))]
    {
        if let Some(armed) = _app.try_state::<QuickUnlock>() {
            *armed.0.lock().unwrap() = None;
        }
        hello::forget();
    }
}

// ── iOS Password AutoFill ─────────────────────────────────────────────────────
//
// See autofill.rs. Enabling needs the vault open, since it publishes the
// current entries; after that, unlocks and vault writes keep it current.

#[derive(serde::Serialize)]
struct AutofillStatus {
    supported: bool,
    enabled: bool,
}

#[tauri::command]
fn autofill_status() -> AutofillStatus {
    #[cfg(target_os = "ios")]
    return AutofillStatus {
        supported: autofill::is_supported(),
        enabled: autofill::is_enabled(),
    };
    #[cfg(not(target_os = "ios"))]
    AutofillStatus { supported: false, enabled: false }
}

#[tauri::command]
async fn set_autofill_enabled(enabled: bool, _state: State<'_, VaultState>) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        if enabled {
            let s = _state.lock().unwrap();
            autofill::publish(s.data.as_ref().ok_or("Vault is locked")?)
        } else {
            autofill::disable();
            Ok(())
        }
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = enabled;
        Err("AutoFill is only available on iOS".into())
    }
}

// ── Lock on suspend / screen lock ─────────────────────────────────────────────
//
// Inactivity auto-lock only fires while the app is running and the user is at
// the machine. Walking away and locking the screen, or closing a laptop lid,
// should scrub the key too. Both are watched by one polling thread rather than
// a Win32 message loop, so the same code works whatever the platform offers.

/// Whether the user has asked for this. Off by default so behaviour only
/// changes for people who turn it on.
#[cfg(desktop)]
struct LockOnSystemEvents(std::sync::atomic::AtomicBool);

#[cfg(desktop)]
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);

/// True when the wall clock jumped much further than the monotonic clock over
/// the same interval, which means the machine was suspended in between.
/// `Instant` is frozen while a Windows machine sleeps; `SystemTime` is not.
#[cfg(desktop)]
fn was_suspended(wall_gap: std::time::Duration, monotonic_gap: std::time::Duration) -> bool {
    // Ordinary scheduling jitter is milliseconds. Anything past this is a gap
    // in which the process was not running at all.
    const TOLERANCE: std::time::Duration = std::time::Duration::from_secs(10);
    wall_gap > monotonic_gap + TOLERANCE
}

/// True when the Windows session is showing the secure desktop, which is what
/// Win+L and the screensaver password prompt switch to. A normal-privilege
/// process cannot open that desktop at all, so a failure here counts as locked.
#[cfg(all(desktop, target_os = "windows"))]
fn screen_is_locked() -> bool {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::StationsAndDesktops::{
        CloseDesktop, GetUserObjectInformationW, OpenInputDesktop, DESKTOP_READOBJECTS,
        DF_ALLOWOTHERACCOUNTHOOK, UOI_NAME,
    };

    unsafe {
        let Ok(desktop) = OpenInputDesktop(DF_ALLOWOTHERACCOUNTHOOK, false, DESKTOP_READOBJECTS)
        else {
            return true;
        };

        let mut buf = [0u16; 256];
        let mut needed = 0u32;
        let ok = GetUserObjectInformationW(
            HANDLE(desktop.0),
            UOI_NAME,
            Some(buf.as_mut_ptr() as *mut _),
            (buf.len() * 2) as u32,
            Some(&mut needed),
        )
        .is_ok();
        let _ = CloseDesktop(desktop);

        if !ok {
            return false;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        // "Default" is the interactive desktop; "Winlogon" and "Screen-saver"
        // are the ones shown when the session is locked.
        !String::from_utf16_lossy(&buf[..len]).eq_ignore_ascii_case("Default")
    }
}

#[cfg(all(desktop, not(target_os = "windows")))]
fn screen_is_locked() -> bool {
    // No detection implemented for this platform yet; suspend is still caught.
    false
}

/// Polls for suspend and screen lock, locking the vault when either happens
/// and the setting is on.
#[cfg(desktop)]
fn watch_for_system_lock(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut last_wall = std::time::SystemTime::now();
        let mut last_monotonic = std::time::Instant::now();
        let mut was_locked = screen_is_locked();

        loop {
            std::thread::sleep(WATCH_INTERVAL);

            let wall_gap = std::time::SystemTime::now()
                .duration_since(last_wall)
                .unwrap_or_default();
            let monotonic_gap = last_monotonic.elapsed();
            last_wall = std::time::SystemTime::now();
            last_monotonic = std::time::Instant::now();

            let locked_now = screen_is_locked();
            // Only the transition counts, or the vault could not be unlocked
            // from the overlay while the screensaver desktop is still up.
            let just_locked = locked_now && !was_locked;
            was_locked = locked_now;

            let enabled = app
                .try_state::<LockOnSystemEvents>()
                .map(|s| s.0.load(std::sync::atomic::Ordering::Relaxed))
                .unwrap_or(false);
            if !enabled {
                continue;
            }

            if just_locked || was_suspended(wall_gap, monotonic_gap) {
                let unlocked = app
                    .try_state::<VaultState>()
                    .map(|s| s.lock().unwrap().key.is_some())
                    .unwrap_or(false);
                if unlocked {
                    lock_and_notify(&app);
                }
            }
        }
    });
}

#[cfg(desktop)]
#[tauri::command]
fn set_lock_on_system_events(enabled: bool, state: State<'_, LockOnSystemEvents>) {
    state.0.store(enabled, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(mobile)]
#[tauri::command]
fn set_lock_on_system_events(enabled: bool) {
    // Mobile locks whenever the app leaves the foreground, driven from the
    // frontend; nothing to poll here.
    let _ = enabled;
}

#[tauri::command]
fn lock(app: tauri::AppHandle) {
    lock_and_notify(&app);
}

#[tauri::command]
fn is_unlocked(state: State<'_, VaultState>) -> bool {
    state.lock().unwrap().key.is_some()
}

#[tauri::command]
fn list_entries(state: State<'_, VaultState>) -> Result<Vec<Entry>, String> {
    let s = state.lock().unwrap();
    s.data
        .as_ref()
        .map(|d| d.entries.clone())
        .ok_or("Vault is locked".into())
}

#[tauri::command]
async fn add_entry(
    name: String,
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    folder_id: Option<String>,
    totp_secret: Option<String>,
    state: State<'_, VaultState>,
) -> Result<Entry, String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_entry(&key, data, name, username, email, password, url, notes, folder_id, totp_secret)
}

#[tauri::command]
async fn update_entry(
    id: String,
    name: String,
    username: Option<String>,
    email: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    folder_id: Option<String>,
    totp_secret: Option<String>,
    state: State<'_, VaultState>,
) -> Result<Entry, String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::update_entry(&key, data, &id, name, username, email, password, url, notes, folder_id, totp_secret)
}

#[tauri::command]
fn list_folders(state: State<'_, VaultState>) -> Result<Vec<Folder>, String> {
    let s = state.lock().unwrap();
    s.data
        .as_ref()
        .map(|d| d.folders.clone())
        .ok_or("Vault is locked".into())
}

#[tauri::command]
async fn add_folder(name: String, state: State<'_, VaultState>) -> Result<Folder, String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::add_folder(&key, data, name)
}

#[tauri::command]
async fn rename_folder(id: String, name: String, state: State<'_, VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::rename_folder(&key, data, &id, name)
}

#[tauri::command]
async fn delete_folder(id: String, state: State<'_, VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::delete_folder(&key, data, &id)
}

#[tauri::command]
async fn mark_entry_used(id: String, state: State<'_, VaultState>) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    vault::mark_entry_used(&key, data, &id)
}

#[tauri::command]
async fn delete_entry(id: String, state: State<'_, VaultState>) -> Result<(), String> {
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
async fn import_vault(app: tauri::AppHandle) -> Result<bool, String> {
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
    lock_and_notify(&app);
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

/// Tracks the last value this app put on the clipboard: a generation counter
/// (so an older scheduled clear doesn't cancel a newer copy) and the text
/// itself (so we never wipe something the user copied from another app in
/// the meantime). The text zeroizes when replaced or dropped.
struct ClipboardClearGen(Mutex<(u64, Option<Zeroizing<String>>)>);

/// Desktop: writes via arboard so sensitive values can be excluded from
/// Windows' clipboard history / cloud sync. Mobile: uses the
/// clipboard-manager plugin (iOS/Android system pasteboard).
#[tauri::command]
fn write_clipboard_text(
    app: tauri::AppHandle,
    state: State<ClipboardClearGen>,
    text: String,
) -> Result<(), String> {
    let text = Zeroizing::new(text);

    #[cfg(desktop)]
    {
        let _ = &app;
        let mut cb = arboard::Clipboard::new().map_err(|e| e.to_string())?;

        #[cfg(target_os = "windows")]
        {
            use arboard::SetExtWindows;
            cb.set().exclude_from_monitoring().text(text.as_str()).map_err(|e| e.to_string())?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            cb.set_text(text.as_str()).map_err(|e| e.to_string())?;
        }
    }

    #[cfg(mobile)]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().write_text(text.as_str()).map_err(|e| e.to_string())?;
    }

    state.0.lock().unwrap().1 = Some(text);
    Ok(())
}

/// Reads the current clipboard text, if any.
fn read_clipboard_text(app: &tauri::AppHandle) -> Option<Zeroizing<String>> {
    #[cfg(desktop)]
    {
        let _ = app;
        arboard::Clipboard::new().ok()?.get_text().ok().map(Zeroizing::new)
    }
    #[cfg(mobile)]
    {
        use tauri_plugin_clipboard_manager::ClipboardExt;
        app.clipboard().read_text().ok().map(Zeroizing::new)
    }
}

/// Clears the clipboard after `seconds`, unless a newer copy has been made
/// in the meantime or the clipboard no longer holds what we put there.
/// Runs on a background thread so it fires reliably even when the window
/// is hidden/unfocused and JS timers get throttled.
#[tauri::command]
fn schedule_clipboard_clear(app: tauri::AppHandle, state: State<ClipboardClearGen>, seconds: u64) {
    let my_gen = {
        let mut g = state.0.lock().unwrap();
        g.0 += 1;
        g.0
    };

    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(seconds));

        let gen_state = app.state::<ClipboardClearGen>();
        let ours = {
            let mut g = gen_state.0.lock().unwrap();
            if g.0 != my_gen {
                return;
            }
            g.1.take()
        };

        // Only clear if the clipboard still contains our value. If the user
        // copied something else since, leave it alone.
        let still_ours = match (ours, read_clipboard_text(&app)) {
            (Some(ours), Some(current)) => *ours == *current,
            _ => false,
        };
        if !still_ours {
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
        let new_sc = parse_shortcut_str(&shortcut_str)?;

        let old_str = shortcut_state.0.lock().unwrap().clone();
        let old_sc = parse_shortcut_str(&old_str).ok();
        if old_sc == Some(new_sc) {
            return Ok(());
        }

        // Unregister the current shortcut, then register the new one. If
        // registration fails (e.g. another app owns that combination), put
        // the old one back so the user is never left without a hotkey.
        if let Some(old) = old_sc {
            let _ = app.global_shortcut().unregister(old);
        }
        if let Err(e) = register_overlay_shortcut(&app, new_sc) {
            if let Some(old) = old_sc {
                let _ = register_overlay_shortcut(&app, old);
            }
            return Err(format!("Could not register shortcut: {e}"));
        }

        *shortcut_state.0.lock().unwrap() = shortcut_str.clone();

        // Persist to config file (the directory may not exist yet)
        let data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
        std::fs::write(data_dir.join(SHORTCUT_FILE), &shortcut_str)
            .map_err(|e| format!("Shortcut active but could not be saved: {e}"))?;

        Ok(())
    }
}

/// Registers `shortcut` to toggle the quick-access overlay on key press only
/// (not release — that would fire the toggle twice on Windows).
#[cfg(desktop)]
fn register_overlay_shortcut(
    app: &tauri::AppHandle,
    shortcut: Shortcut,
) -> Result<(), tauri_plugin_global_shortcut::Error> {
    let app_handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state() == SCState::Pressed {
                toggle_overlay(&app_handle);
            }
        })
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

    // Fall back to the default if the persisted string is invalid, and make
    // the managed state reflect what is actually registered.
    let (shortcut_str, shortcut) = match parse_shortcut_str(shortcut_str.trim()) {
        Ok(sc) => (shortcut_str.trim().to_string(), sc),
        Err(_) => (
            DEFAULT_SHORTCUT.to_string(),
            Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP),
        ),
    };
    *app.state::<OverlayShortcut>().0.lock().unwrap() = shortcut_str;

    register_overlay_shortcut(app.handle(), shortcut)?;

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
                lock_and_notify(app);
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
        .manage(ClipboardClearGen(Mutex::new((0, None))))
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
            set_lock_on_system_events,
            quick_unlock_available,
            quick_unlock_armed,
            arm_quick_unlock,
            quick_unlock,
            disarm_quick_unlock,
            autofill_status,
            set_autofill_enabled,
        ])
        .setup(|app| {
            // On mobile, store the vault inside the app sandbox
            // (iOS: <container>/Library/Application Support). Desktop keeps
            // its historical location so existing vaults still load.
            #[cfg(mobile)]
            vault::set_vault_dir(app.path().app_data_dir()?);

            #[cfg(desktop)]
            {
                setup_desktop(app)?;
                app.manage(LockOnSystemEvents(std::sync::atomic::AtomicBool::new(false)));
                #[cfg(target_os = "windows")]
                app.manage(QuickUnlock(Mutex::new(None)));
                watch_for_system_lock(app.handle().clone());
            }

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

#[cfg(all(test, desktop))]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn ordinary_ticks_are_not_mistaken_for_suspend() {
        // The two clocks track each other while the process is running, give
        // or take scheduling jitter
        assert!(!was_suspended(Duration::from_secs(2), Duration::from_secs(2)));
        assert!(!was_suspended(Duration::from_millis(2300), Duration::from_secs(2)));
        assert!(!was_suspended(Duration::from_secs(11), Duration::from_secs(2)));
    }

    #[test]
    fn a_wall_clock_jump_is_read_as_suspend() {
        // Laptop lid closed for an hour: the monotonic clock barely moved
        assert!(was_suspended(Duration::from_secs(3600), Duration::from_secs(2)));
        assert!(was_suspended(Duration::from_secs(60), Duration::from_secs(2)));
    }

    #[test]
    fn a_backwards_wall_clock_is_not_a_suspend() {
        // A clock correction can make the gap zero or negative; saturating
        // subtraction leaves it at zero, which must not trigger a lock
        assert!(!was_suspended(Duration::ZERO, Duration::from_secs(2)));
    }

    #[test]
    fn checking_the_desktop_does_not_panic() {
        // Value depends on whether the screen is locked right now, so only
        // the call itself is under test
        let _ = screen_is_locked();
    }
}
