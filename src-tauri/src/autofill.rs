// iOS Password AutoFill.
//
// Everything Apple-specific lives in Swift (src-tauri/ios/): the App Group
// container, the Keychain, the credential identity store and the AutoFill
// extension itself. Rust only decides *what* the extension may see and hands
// it over as JSON through the C functions below, which are exported by
// src-tauri/ios/App/AutoFillBridge.swift and resolved when the app links.
//
// The extension never touches vault.enc or the master key. Each publish, the
// Swift side encrypts this snapshot under a fresh random key, writes it to the
// shared container, and stores that key in the Keychain behind Face ID /
// passcode. AutoFill is "enabled" exactly when that snapshot file exists.

use serde::Serialize;
use zeroize::Zeroizing;

use crate::vault::VaultData;

extern "C" {
    fn vault_autofill_is_supported() -> bool;
    fn vault_autofill_is_enabled() -> bool;
    fn vault_autofill_publish(json: *const u8, len: usize) -> i32;
    fn vault_autofill_disable();
}

/// Only the fields a login form needs. Notes, history and folders stay out.
#[derive(Serialize)]
struct Snapshot<'a> {
    entries: Vec<SnapshotEntry<'a>>,
}

#[derive(Serialize)]
struct SnapshotEntry<'a> {
    id: &'a str,
    name: &'a str,
    /// Username if set, else email: whichever the entry signs in with
    login: &'a str,
    password: &'a str,
    url: Option<&'a str>,
    totp_secret: Option<&'a str>,
}

/// False when the build lacks the shared App Group (a simulator run, or a
/// sideload that stripped the entitlement). The UI hides the option then.
pub fn is_supported() -> bool {
    unsafe { vault_autofill_is_supported() }
}

pub fn is_enabled() -> bool {
    unsafe { vault_autofill_is_enabled() }
}

/// Writes the snapshot and registers its logins with iOS. Also how AutoFill
/// is turned on.
pub fn publish(data: &VaultData) -> Result<(), String> {
    let snapshot = Snapshot {
        entries: data
            .entries
            .iter()
            .map(|e| SnapshotEntry {
                id: &e.id,
                name: &e.name,
                login: e.username.as_deref().filter(|u| !u.is_empty()).unwrap_or(&e.email),
                password: &e.password,
                url: e.url.as_deref().filter(|u| !u.is_empty()),
                totp_secret: e.totp_secret.as_deref().filter(|s| !s.is_empty()),
            })
            .collect(),
    };
    let json = Zeroizing::new(serde_json::to_vec(&snapshot).map_err(|e| e.to_string())?);

    match unsafe { vault_autofill_publish(json.as_ptr(), json.len()) } {
        0 => Ok(()),
        1 => Err("This build of Vault can't share data with its AutoFill extension.".into()),
        2 => Err("Set a device passcode to use AutoFill.".into()),
        _ => Err("AutoFill could not be updated.".into()),
    }
}

/// Keeps AutoFill in step after an unlock or a vault write. A no-op unless
/// the user turned it on. Errors are logged, not returned: the vault write
/// they follow has already succeeded and must not be reported as failed.
pub fn refresh(data: &VaultData) {
    if is_enabled() {
        if let Err(e) = publish(data) {
            eprintln!("AutoFill refresh failed: {e}");
        }
    }
}

/// Deletes the snapshot and its key, and removes Vault's logins from iOS.
pub fn disable() {
    unsafe { vault_autofill_disable() }
}
