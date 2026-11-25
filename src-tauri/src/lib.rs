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
mod registry;
mod vault;

use registry::Registry;
use std::path::{Path, PathBuf};
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
async fn create_vault(
    mut password: String,
    state: State<'_, VaultState>,
    sync: State<'_, SyncFolder>,
) -> Result<(), String> {
    // A sync folder that isn't there (an unmounted drive, a sync client not
    // signed in) must not get a fresh empty vault that later clashes with the
    // real one when it reappears.
    if sync.lock().unwrap().folder.as_deref().is_some_and(|f| !f.is_dir()) {
        password.zeroize();
        return Err("Your sync folder isn't available. Reconnect it and restart Vault.".into());
    }
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
    let (key, mut data) = result?;
    // Folds in conflict copies a sync client left while the vault was locked
    let _ = vault::sync(&key, &mut data);
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
    let result = match (&s.key, &mut s.data) {
        (Some(key), Some(data)) => {
            // Conflict copies are under the old key; fold them in while it still opens them
            let _ = vault::sync(key, data);
            vault::change_master_password(&current_password, &new_password, data)
        }
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
        // The store's cipher holds a copy of the key too
        vault::close_store();
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

/// Wrapped keys by vault id: each vault's key opens only that vault, and
/// switching away and back shouldn't cost the user their Hello unlock.
#[cfg(all(desktop, target_os = "windows"))]
struct QuickUnlock(Mutex<std::collections::HashMap<String, ArmedQuickUnlock>>);

/// The id of the vault the lock screen and every command work on.
fn active_vault_id(app: &tauri::AppHandle) -> Option<String> {
    app.try_state::<Vaults>().map(|v| v.lock().unwrap().active.clone())
}

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
    return match (_app.try_state::<QuickUnlock>(), active_vault_id(&_app)) {
        (Some(s), Some(id)) => s.0.lock().unwrap().contains_key(&id),
        _ => false,
    };
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
        let (vault_id, key) = {
            let s = state.lock().unwrap();
            let key = s.key.as_deref().ok_or("Vault is locked")?;
            // Read under the session lock, so a switch can't slip in between
            let id = active_vault_id(&_app).ok_or("Vault is locked")?;
            (id, Zeroizing::new(*key))
        };

        let mut challenge = [0u8; 32];
        {
            use rand::RngCore;
            rand::thread_rng().fill_bytes(&mut challenge);
        }

        let wrapping_key = hello::derive_wrapping_key(&challenge)?;
        let wrapped_key = crypto::encrypt(&key[..], &wrapping_key)?;

        let armed = _app.try_state::<QuickUnlock>().ok_or("Quick unlock unavailable")?;
        armed.0.lock().unwrap().insert(vault_id, ArmedQuickUnlock { challenge, wrapped_key });
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
        let vault_id = active_vault_id(&_app).ok_or("Vault is unavailable")?;
        let (challenge, wrapped_key) = {
            let guard = armed.0.lock().unwrap();
            let a = guard.get(&vault_id).ok_or("Quick unlock is not set up")?;
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
        let mut data = vault::unlock_with_key(&key).map_err(|_| {
            "The vault has changed since quick unlock was set up. Use your master password."
                .to_string()
        })?;
        let _ = vault::sync(&key, &mut data);

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
            armed.0.lock().unwrap().clear();
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
    /// On, and showing this vault's logins
    enabled: bool,
    /// On, but showing another vault's; turning it on here moves it
    other_vault: Option<String>,
}

#[tauri::command]
fn autofill_status(_vaults: State<'_, Vaults>) -> AutofillStatus {
    #[cfg(target_os = "ios")]
    {
        let on = autofill::is_enabled();
        let reg = _vaults.lock().unwrap();
        let mine = reg.autofill.as_deref() == Some(reg.active.as_str());
        return AutofillStatus {
            supported: autofill::is_supported(),
            enabled: on && mine,
            other_vault: if on && !mine {
                reg.autofill.as_deref().and_then(|id| reg.get(id)).map(|v| v.name.clone())
            } else {
                None
            },
        };
    }
    #[cfg(not(target_os = "ios"))]
    AutofillStatus { supported: false, enabled: false, other_vault: None }
}

#[tauri::command]
async fn set_autofill_enabled(
    enabled: bool,
    _state: State<'_, VaultState>,
    _vaults: State<'_, Vaults>,
) -> Result<(), String> {
    #[cfg(target_os = "ios")]
    {
        if enabled {
            let s = _state.lock().unwrap();
            autofill::publish(s.data.as_ref().ok_or("Vault is locked")?)?;
            let mut reg = _vaults.lock().unwrap();
            reg.autofill = Some(reg.active.clone());
            autofill::set_active_is_source(true);
            reg.save()
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
        .map(VaultData::entry_views)
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
        .map(VaultData::folder_views)
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
    import_csv_from_path(&path, &state).map(Some)
}

/// Reads and parses a CSV export; the raw content is zeroized on drop.
fn read_csv_file(path: &Path) -> Result<csv_import::CsvParseResult, String> {
    let raw = Zeroizing::new(
        std::fs::read_to_string(path).map_err(|_| "Cannot read the selected file".to_string())?,
    );
    csv_import::parse(&raw)
}

fn import_csv_from_path(path: &Path, state: &VaultState) -> Result<CsvImportReport, String> {
    let parsed = read_csv_file(path)?;
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let key = s.key.as_deref().ok_or("Vault is locked")?;
    let data = s.data.as_mut().ok_or("Vault is locked")?;
    let imported = vault::import_csv_logins(&key, data, parsed.logins)?;
    Ok(CsvImportReport { imported, skipped: parsed.skipped })
}

/// A CSV picked on the first-run screen, before a vault (and so a key)
/// exists. Only the path is held; the file is re-read once the vault is
/// created, so no plaintext logins sit in memory while the user types.
#[derive(Default)]
struct PendingCsvImport(Mutex<Option<PathBuf>>);

/// Picks and validates a CSV to import after the vault is created. Returns
/// how many logins it holds, or None when the user cancels.
#[tauri::command]
async fn stage_csv_import(
    app: tauri::AppHandle,
    pending: State<'_, PendingCsvImport>,
) -> Result<Option<usize>, String> {
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
    let count = read_csv_file(&path)?.logins.len();
    if count == 0 {
        return Err("No logins found in that file".into());
    }
    *pending.0.lock().unwrap() = Some(path);
    Ok(Some(count))
}

#[tauri::command]
fn clear_staged_csv_import(pending: State<'_, PendingCsvImport>) {
    *pending.0.lock().unwrap() = None;
}

/// Imports the staged CSV into the (now unlocked) vault. None if nothing
/// was staged.
#[tauri::command]
async fn apply_staged_csv_import(
    state: State<'_, VaultState>,
    pending: State<'_, PendingCsvImport>,
) -> Result<Option<CsvImportReport>, String> {
    let Some(path) = pending.0.lock().unwrap().take() else {
        return Ok(None);
    };
    import_csv_from_path(&path, &state).map(Some)
}

// ── Sync folder ───────────────────────────────────────────────────────────────
//
// See vault.rs. Desktop only for now: the vault file moves into a folder a
// sync client (OneDrive, Dropbox, iCloud Drive, Syncthing) already watches,
// and a background thread merges in whatever other devices write there.
// Each vault remembers its own folder in the vault list (registry.rs).

/// The active vault's sync folder, if any, and a folder the user picked
/// that already holds another device's vault and is waiting for its password.
#[derive(Default)]
#[cfg_attr(mobile, allow(dead_code))]
struct SyncState {
    folder: Option<PathBuf>,
    pending_join: Option<PathBuf>,
}

type SyncFolder = Mutex<SyncState>;

/// Records the active vault's sync folder in the vault list.
#[cfg_attr(mobile, allow(dead_code))]
fn save_sync_folder(app: &tauri::AppHandle, folder: Option<&Path>) -> Result<(), String> {
    let vaults = app.state::<Vaults>();
    let mut reg = vaults.lock().unwrap();
    let id = reg.active.clone();
    reg.set_sync_folder(&id, folder.map(Path::to_path_buf))?;
    reg.save()
}

/// Refuses a sync folder another vault on this device already keeps its
/// file in: the two would merge into each other.
#[cfg_attr(mobile, allow(dead_code))]
fn check_folder_unused(app: &tauri::AppHandle, dir: &Path) -> Result<(), String> {
    let vaults = app.state::<Vaults>();
    let reg = vaults.lock().unwrap();
    match reg.using_folder(dir, &reg.active) {
        Some(other) => Err(format!("Your vault \u{201c}{}\u{201d} already syncs through that folder. Choose a different one.", other.name)),
        None => Ok(()),
    }
}

#[derive(serde::Serialize)]
struct SyncStatus {
    supported: bool,
    folder: Option<String>,
    /// The folder is configured but not there right now
    missing: bool,
}

#[tauri::command]
fn sync_status(sync: State<'_, SyncFolder>) -> SyncStatus {
    let s = sync.lock().unwrap();
    SyncStatus {
        supported: cfg!(desktop),
        folder: s.folder.as_ref().map(|f| f.display().to_string()),
        missing: s.folder.as_deref().is_some_and(|f| !f.is_dir()),
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(mobile, allow(dead_code))]
enum SyncSetup {
    Cancelled,
    /// The vault moved into the folder and is now syncing
    Moved,
    /// The folder already has a vault; `join_sync_folder` needs its password
    NeedsPassword,
}

/// Asks for a folder and moves the vault into it. If another device's vault
/// is already there, nothing moves yet: the UI asks for that vault's master
/// password and calls `join_sync_folder`.
#[cfg(desktop)]
#[tauri::command]
async fn choose_sync_folder(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    sync: State<'_, SyncFolder>,
) -> Result<SyncSetup, String> {
    use tauri_plugin_dialog::DialogExt;
    if state.lock().unwrap().key.is_none() {
        return Err("Vault is locked".into());
    }
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose a folder your sync app keeps up to date")
        .blocking_pick_folder()
    else {
        return Ok(SyncSetup::Cancelled);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    check_folder_unused(&app, &dir)?;

    if dir.join("vault.enc").exists() {
        sync.lock().unwrap().pending_join = Some(dir);
        return Ok(SyncSetup::NeedsPassword);
    }

    // Held so no save lands halfway through the move
    let _guard = state.lock().unwrap();
    let previous = sync.lock().unwrap().folder.clone();
    // Config first: if Vault quit right after the move, it must find the vault
    save_sync_folder(&app, Some(&dir))?;
    if let Err(e) = vault::move_vault_to(&dir) {
        let _ = save_sync_folder(&app, previous.as_deref());
        return Err(e);
    }
    let mut s = sync.lock().unwrap();
    s.folder = Some(dir);
    s.pending_join = None;
    Ok(SyncSetup::Moved)
}

#[cfg(mobile)]
#[tauri::command]
async fn choose_sync_folder() -> Result<SyncSetup, String> {
    Err("Sync folders are not available on mobile yet".into())
}

/// Joins the vault another device keeps in the folder picked by
/// `choose_sync_folder`, merging this device's entries into it. From then on
/// this device uses that vault and its master password.
#[cfg(desktop)]
#[tauri::command]
async fn join_sync_folder(
    app: tauri::AppHandle,
    mut password: String,
    state: State<'_, VaultState>,
    sync: State<'_, SyncFolder>,
) -> Result<(), String> {
    let (dir, previous) = {
        let s = sync.lock().unwrap();
        (s.pending_join.clone(), s.folder.clone())
    };
    let Some(dir) = dir else {
        password.zeroize();
        return Err("Choose a sync folder first".into());
    };

    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    let Some(data) = s.data.as_mut() else {
        password.zeroize();
        return Err("Vault is locked".into());
    };

    if let Err(e) = save_sync_folder(&app, Some(&dir)) {
        password.zeroize();
        return Err(e);
    }
    let result = vault::join_vault_in(&dir, &password, data);
    password.zeroize();
    let key = match result {
        Ok(key) => key,
        Err(e) => {
            let _ = save_sync_folder(&app, previous.as_deref());
            return Err(e);
        }
    };
    // Replacing the old key zeroizes it on drop
    s.key = Some(key);
    drop(guard);

    // A wrapped copy of the old key no longer opens anything; the next unlock re-arms it
    #[cfg(target_os = "windows")]
    if let (Some(armed), Some(id)) = (app.try_state::<QuickUnlock>(), active_vault_id(&app)) {
        armed.0.lock().unwrap().remove(&id);
    }

    let mut s = sync.lock().unwrap();
    s.folder = Some(dir);
    s.pending_join = None;
    drop(s);
    let _ = app.emit("vault-changed", ());
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
async fn join_sync_folder(mut password: String) -> Result<(), String> {
    password.zeroize();
    Err("Sync folders are not available on mobile yet".into())
}

/// For a device with no vault yet: points it at the vault another device
/// keeps in a sync folder. Returns false if the dialog is cancelled.
#[cfg(desktop)]
#[tauri::command]
async fn open_synced_vault(app: tauri::AppHandle, sync: State<'_, SyncFolder>) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    if vault::vault_exists() {
        return Err("This vault already exists. Join the synced one from Settings.".into());
    }
    let Some(picked) = app
        .dialog()
        .file()
        .set_title("Choose the folder that holds your synced vault")
        .blocking_pick_folder()
    else {
        return Ok(false);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    check_folder_unused(&app, &dir)?;

    let previous = sync.lock().unwrap().folder.clone();
    save_sync_folder(&app, Some(&dir))?;
    if let Err(e) = vault::open_vault_in(&dir) {
        let _ = save_sync_folder(&app, previous.as_deref());
        return Err(e);
    }
    sync.lock().unwrap().folder = Some(dir);
    Ok(true)
}

#[cfg(mobile)]
#[tauri::command]
async fn open_synced_vault() -> Result<bool, String> {
    Err("Sync folders are not available on mobile yet".into())
}

/// Moves the vault back out of the sync folder onto this device only. The
/// copy in the sync folder stays there for the other devices.
#[cfg(desktop)]
#[tauri::command]
async fn stop_sync(
    app: tauri::AppHandle,
    state: State<'_, VaultState>,
    sync: State<'_, SyncFolder>,
) -> Result<(), String> {
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    // Take the latest from the other devices along
    let changed = match (&s.key, &mut s.data) {
        (Some(key), Some(data)) => vault::sync(key, data)?,
        _ => false,
    };

    let previous = sync.lock().unwrap().folder.clone();
    let home = app.state::<Vaults>().lock().unwrap().active().home.clone();
    save_sync_folder(&app, None)?;
    if let Err(e) = vault::move_vault_home(&home) {
        let _ = save_sync_folder(&app, previous.as_deref());
        return Err(e);
    }
    drop(guard);
    sync.lock().unwrap().folder = None;
    if changed {
        let _ = app.emit("vault-changed", ());
    }
    Ok(())
}

#[cfg(mobile)]
#[tauri::command]
async fn stop_sync() -> Result<(), String> {
    Err("Sync folders are not available on mobile yet".into())
}

/// How often an unlocked vault checks its sync folder for other devices' writes.
#[cfg(desktop)]
const SYNC_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

/// Merges other devices' changes in while the vault is open, and tells the
/// windows to reload when there were any.
#[cfg(desktop)]
fn watch_sync_folder(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(SYNC_INTERVAL);

        if app.state::<SyncFolder>().lock().unwrap().folder.is_none() {
            continue;
        }
        let outcome = {
            let state = app.state::<VaultState>();
            let mut guard = state.lock().unwrap();
            let s = &mut *guard;
            match (&s.key, &mut s.data) {
                (Some(key), Some(data)) => Some(vault::sync(key, data)),
                _ => None,
            }
        };
        match outcome {
            Some(Ok(true)) => {
                let _ = app.emit("vault-changed", ());
            }
            // The session key is stale; every save would now fail
            Some(Err(e)) if e == vault::KEY_CHANGED => {
                lock_and_notify(&app);
                let _ = app.emit("vault-sync-error", e);
            }
            // Mid-upload, file locked, etc.: try again next tick
            _ => {}
        }
    });
}

// ── Several vaults ────────────────────────────────────────────────────────────
//
// See registry.rs. One vault is active at a time; the commands above all act
// on it. Switching locks the open vault first, so two people sharing a device
// never see each other's entries, and a key is never used on the wrong file.

type Vaults = Mutex<Registry>;

#[derive(serde::Serialize)]
struct VaultSummary {
    id: String,
    name: String,
    /// False until a master password has been set for it
    exists: bool,
    synced: bool,
}

#[derive(serde::Serialize)]
struct VaultList {
    active: String,
    vaults: Vec<VaultSummary>,
}

#[tauri::command]
fn list_vaults(vaults: State<'_, Vaults>) -> VaultList {
    let reg = vaults.lock().unwrap();
    VaultList {
        active: reg.active.clone(),
        vaults: reg
            .vaults
            .iter()
            .map(|v| VaultSummary {
                id: v.id.clone(),
                name: v.name.clone(),
                exists: v.exists(),
                synced: v.sync_folder.is_some(),
            })
            .collect(),
    }
}

/// Points the vault file, the sync watcher and AutoFill at the active vault.
fn apply_active(app: &tauri::AppHandle, reg: &Registry) {
    let active = reg.active();
    vault::set_vault_dir(active.dir().to_path_buf());
    if let Some(sync) = app.try_state::<SyncFolder>() {
        let mut s = sync.lock().unwrap();
        s.folder = active.sync_folder.clone();
        s.pending_join = None;
    }
    #[cfg(target_os = "ios")]
    autofill::set_active_is_source(reg.autofill.as_deref() == Some(reg.active.as_str()));
}

/// Locks whatever is open, then makes `id` the active vault.
fn activate(app: &tauri::AppHandle, id: &str) -> Result<(), String> {
    lock_and_notify(app);
    // A CSV picked for one vault's first run must not land in another
    if let Some(pending) = app.try_state::<PendingCsvImport>() {
        *pending.0.lock().unwrap() = None;
    }
    let vaults = app.state::<Vaults>();
    let mut reg = vaults.lock().unwrap();
    let previous = reg.active.clone();
    reg.set_active(id)?;
    if let Err(e) = reg.save() {
        reg.active = previous;
        return Err(e);
    }
    apply_active(app, &reg);
    drop(reg);
    let _ = app.emit("vault-switched", ());
    let _ = app.emit("vault-list-changed", ());
    Ok(())
}

fn is_unlocked_now(app: &tauri::AppHandle) -> bool {
    app.state::<VaultState>().lock().unwrap().key.is_some()
}

#[tauri::command]
async fn switch_vault(app: tauri::AppHandle, id: String) -> Result<(), String> {
    if active_vault_id(&app).as_deref() == Some(id.as_str()) {
        return Ok(());
    }
    activate(&app, &id)
}

/// Lists a new vault and switches to it. The lock screen then asks for its
/// master password, which creates the file. Returns the new vault's id.
#[tauri::command]
async fn add_vault(app: tauri::AppHandle, name: String) -> Result<String, String> {
    let id = {
        let vaults = app.state::<Vaults>();
        let mut reg = vaults.lock().unwrap();
        let id = reg.add(&name)?;
        if let Err(e) = reg.save() {
            reg.vaults.retain(|v| v.id != id);
            return Err(e);
        }
        id
    };
    activate(&app, &id)?;
    Ok(id)
}

/// Renames the open vault. Only its own session can, so one person can't
/// relabel another's vault from the lock screen.
#[tauri::command]
async fn rename_vault(app: tauri::AppHandle, id: String, name: String) -> Result<(), String> {
    if active_vault_id(&app).as_deref() != Some(id.as_str()) || !is_unlocked_now(&app) {
        return Err("Unlock that vault to rename it".into());
    }
    let vaults = app.state::<Vaults>();
    let mut reg = vaults.lock().unwrap();
    let previous = reg.get(&id).map(|v| v.name.clone()).ok_or("No such vault")?;
    reg.rename(&id, &name)?;
    if let Err(e) = reg.save() {
        let _ = reg.rename(&id, &previous);
        return Err(e);
    }
    drop(reg);
    let _ = app.emit("vault-list-changed", ());
    Ok(())
}

/// Takes a vault off this device: drops it from the list and deletes its
/// local file. A copy in a sync folder is left for the other devices. A vault
/// that has a file can only be removed from its own unlocked session; one
/// that was added but never given a password can go from the lock screen.
#[tauri::command]
async fn remove_vault(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let (is_active, exists) = {
        let reg = app.state::<Vaults>();
        let reg = reg.lock().unwrap();
        let v = reg.get(&id).ok_or("No such vault")?;
        (reg.active == id, v.exists())
    };
    if exists && !(is_active && is_unlocked_now(&app)) {
        return Err("Unlock that vault to remove it".into());
    }
    if is_active {
        lock_and_notify(&app);
    }

    let vaults = app.state::<Vaults>();
    let mut reg = vaults.lock().unwrap();
    #[cfg(target_os = "ios")]
    let fed_autofill = reg.autofill.as_deref() == Some(id.as_str());
    let removed = reg.remove(&id)?;
    let owns_home = reg.owns_home(&removed.home);
    reg.save()?;
    apply_active(&app, &reg);
    drop(reg);

    #[cfg(all(desktop, target_os = "windows"))]
    if let Some(armed) = app.try_state::<QuickUnlock>() {
        armed.0.lock().unwrap().remove(&id);
    }
    #[cfg(target_os = "ios")]
    if fed_autofill {
        autofill::disable();
    }
    if is_active {
        let _ = app.emit("vault-switched", ());
    }
    let _ = app.emit("vault-list-changed", ());

    // Only this device's copies. The original vault's folder may hold other
    // things, so just its files go; a folder made for the vault goes whole.
    let result = if owns_home {
        match std::fs::remove_dir_all(&removed.home) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        ["vault.enc", "vault.enc.bak", "vault.enc.pre-sync"]
            .iter()
            .map(|f| std::fs::remove_file(removed.home.join(f)))
            .find(|r| matches!(r, Err(e) if e.kind() != std::io::ErrorKind::NotFound))
            .unwrap_or(Ok(()))
    };
    result.map_err(|e| format!("The vault was removed from the list, but its file couldn't be deleted: {e}"))
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

/// Hides the quick-access overlay and brings the main window to the front.
#[tauri::command]
fn open_main_window(app: tauri::AppHandle) {
    #[cfg(desktop)]
    {
        if let Some(overlay) = app.get_webview_window("overlay") {
            let _ = overlay.hide();
        }
        if let Some(w) = app.get_webview_window("main") {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
        }
    }
    #[cfg(mobile)]
    let _ = app;
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
        .manage(SyncFolder::default())
        .manage(PendingCsvImport::default())
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
            stage_csv_import,
            clear_staged_csv_import,
            apply_staged_csv_import,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            get_overlay_shortcut,
            set_overlay_shortcut,
            open_main_window,
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
            sync_status,
            choose_sync_folder,
            join_sync_folder,
            open_synced_vault,
            stop_sync,
            list_vaults,
            switch_vault,
            add_vault,
            rename_vault,
            remove_vault,
        ])
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            // The device id lives with the app's own data, never in the
            // sync folder the vault may move to. All vaults share it.
            vault::set_device_dir(&data_dir);

            // Where the one vault of earlier versions lives, which becomes
            // the first in the list. On mobile that is the app sandbox
            // (iOS: <container>/Library/Application Support); on desktop the
            // historical location, so existing vaults still load.
            #[cfg(mobile)]
            let first_home = data_dir.clone();
            #[cfg(desktop)]
            let first_home = vault::default_vault_dir().unwrap_or_else(|_| data_dir.join("vault"));

            #[allow(unused_mut)]
            let mut registry = Registry::load(&data_dir, first_home);
            // AutoFill turned on before there was a list came from the only vault
            #[cfg(target_os = "ios")]
            if registry.autofill.is_none() && autofill::is_enabled() {
                registry.autofill = Some(registry.active.clone());
                let _ = registry.save();
            }
            // Points Vault at the active vault's folder, its sync folder
            // included: set even when that folder is missing right now, so
            // Vault never quietly falls back to a stale local copy
            apply_active(app.handle(), &registry);
            app.manage::<Vaults>(Mutex::new(registry));

            #[cfg(desktop)]
            {
                setup_desktop(app)?;
                app.manage(LockOnSystemEvents(std::sync::atomic::AtomicBool::new(false)));
                #[cfg(target_os = "windows")]
                app.manage(QuickUnlock(Mutex::new(Default::default())));
                watch_for_system_lock(app.handle().clone());
                watch_sync_folder(app.handle().clone());
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
