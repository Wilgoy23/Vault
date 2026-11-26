// Syncing a vault through a WebDAV server, on every platform.
//
// The vault file stays on this device, read and written exactly as without
// sync, so Vault works offline and opens without the network. The server
// keeps a copy that each device merges with its own: a background thread
// downloads what the other devices wrote, merges it in and uploads the result
// (folder-crdt's RemoteStore, which retries on top of the other device's file
// whenever another upload got in first). Only the encrypted file reaches the
// server, under the vault's own key.
//
// The account password is kept in the vault list sealed under the vault's
// key, so it can only be read, and the sync only runs, while the vault is
// unlocked.
//
// Locks: the session (VaultState) is never held while waiting on the network,
// and never taken while this module's `store` lock is held.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use folder_crdt::webdav::WebDav;
use folder_crdt::{Fetched, Remote, RemoteStore, Store, StoreError};
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

use crate::registry::{normalize_url, WebDavConfig};
use crate::{crypto, vault, VaultState, Vaults};

/// The vault's file name in the server folder, as in a sync folder.
const FILE_NAME: &str = "vault.enc";
/// How often an unlocked vault checks the server when nothing was saved.
const INTERVAL: Duration = Duration::from_secs(30);
/// After a save, how long to wait for more edits before uploading.
const SETTLE: Duration = Duration::from_millis(800);
/// Every this many syncs, download the file whole instead of trusting the
/// server to say it hasn't changed (see RemoteStore on coarse ETags).
const RECHECK_EVERY: u32 = 10;

type VaultRemote = RemoteStore<vault::RemoteCodec, WebDav>;

/// How the last sync went, for Settings.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Health {
    /// Unix seconds of the last sync that reached the server
    last_sync: Option<u64>,
    error: Option<String>,
    /// The server's file no longer opens with this vault's key: its master
    /// password was changed on another device
    password_changed: bool,
}

/// A server that already holds a vault this one's key doesn't open, waiting
/// for that vault's master password.
struct PendingJoin {
    config: WebDavConfig,
    password: Zeroizing<String>,
    bytes: Vec<u8>,
}

#[derive(Default)]
pub struct WebDavSync {
    /// The open vault's server, while it is unlocked
    store: Mutex<Option<VaultRemote>>,
    health: Mutex<Health>,
    join: Mutex<Option<PendingJoin>>,
    /// The account password for a vault just opened from a server, until
    /// its first unlock seals it into the vault list
    unsealed: Mutex<Option<Zeroizing<String>>>,
}

/// Set by a save, so the thread uploads now rather than at the next tick.
static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Asks for a sync soon. Cheap, and harmless when nothing syncs.
pub fn kick() {
    let (pending, cv) = &WAKE;
    *pending.lock().unwrap() = true;
    cv.notify_one();
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A store error in words for Settings.
fn describe(e: StoreError) -> String {
    match e {
        StoreError::KeyMismatch => "The master password was changed on another device. Enter the new one below to keep syncing.".into(),
        StoreError::Io(e) => {
            let text = e.to_string();
            let mut chars = text.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => "Couldn't reach the server".into(),
            }
        }
        e => format!("The vault on the server can't be read: {e}"),
    }
}

fn set_health(app: &AppHandle, health: Health) {
    if let Some(sync) = app.try_state::<WebDavSync>() {
        *sync.health.lock().unwrap() = health;
    }
    let _ = app.emit("webdav-status", ());
}

fn set_error(app: &AppHandle, error: String) {
    let last_sync = app.try_state::<WebDavSync>().and_then(|s| s.health.lock().unwrap().last_sync);
    set_health(app, Health { last_sync, error: Some(error), password_changed: false });
}

fn remote(config: &WebDavConfig, password: &str) -> Result<WebDav, String> {
    WebDav::new(&config.url, FILE_NAME, &config.username, password).map_err(describe)
}

fn seal(password: &str, key: &[u8; 32]) -> Result<String, String> {
    crypto::encrypt(password.as_bytes(), key)
}

fn unseal(sealed: &str, key: &[u8; 32]) -> Option<Zeroizing<String>> {
    let bytes = crypto::decrypt(sealed, key).ok()?;
    String::from_utf8(bytes.to_vec()).ok().map(Zeroizing::new)
}

/// The open vault's key and a copy of its data, or None while locked.
fn snapshot(app: &AppHandle) -> Option<(Zeroizing<[u8; 32]>, vault::VaultData)> {
    let state = app.state::<VaultState>();
    let s = state.lock().unwrap();
    match (&s.key, &s.data) {
        (Some(key), Some(data)) => Some((key.clone(), data.clone())),
        _ => None,
    }
}

/// Merges what a sync brought into the open vault and saves it here, unless
/// the vault was locked or switched meanwhile.
fn merge_back(app: &AppHandle, key: &[u8; 32], synced: vault::VaultData) {
    let state = app.state::<VaultState>();
    let mut guard = state.lock().unwrap();
    let s = &mut *guard;
    if let (Some(k), Some(data)) = (&s.key, &mut s.data) {
        if **k == *key {
            use folder_crdt::Merge;
            data.merge(synced);
            if let Err(e) = vault::save_vault(k, data) {
                eprintln!("Could not save what the server sync brought in: {e}");
            }
            drop(guard);
            let _ = app.emit("vault-changed", ());
        }
    }
}

fn save_config(app: &AppHandle, config: Option<WebDavConfig>) -> Result<(), String> {
    let vaults = app.state::<Vaults>();
    let mut reg = vaults.lock().unwrap();
    let id = reg.active.clone();
    let previous = reg.active().webdav.clone();
    reg.set_webdav(&id, config)?;
    if let Err(e) = reg.save() {
        let _ = reg.set_webdav(&id, previous);
        return Err(e);
    }
    drop(reg);
    let _ = app.emit("vault-list-changed", ());
    Ok(())
}

/// Refuses a server folder for the active vault when it already syncs
/// through a sync folder, or another vault here uses the same server folder.
fn check_server_free(app: &AppHandle, url: &str) -> Result<(), String> {
    let vaults = app.state::<Vaults>();
    let reg = vaults.lock().unwrap();
    if reg.active().sync_folder.is_some() {
        return Err("This vault syncs through a folder. Stop that first: a vault syncs one way at a time.".into());
    }
    match reg.using_server(url, &reg.active) {
        Some(other) => Err(format!(
            "Your vault \u{201c}{}\u{201d} already syncs through that server folder. Use a different folder.",
            other.name
        )),
        None => Ok(()),
    }
}

// ── The background sync ───────────────────────────────────────────────────────

/// Syncs the open vault every INTERVAL, and shortly after every save.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ticks: u32 = 0;
        loop {
            let kicked = {
                let (pending, cv) = &WAKE;
                let guard = pending.lock().unwrap();
                let (mut guard, _) = cv.wait_timeout_while(guard, INTERVAL, |p| !*p).unwrap();
                std::mem::take(&mut *guard)
            };
            if kicked {
                // Let a burst of edits (an import, a quick fix) go up as one
                std::thread::sleep(SETTLE);
                *WAKE.0.lock().unwrap() = false;
            }
            ticks = ticks.wrapping_add(1);
            sync_now(&app, ticks.is_multiple_of(RECHECK_EVERY));
        }
    });
}

/// Brings the open vault and the server into step, if it syncs through one.
/// Returns without doing anything while locked.
pub fn sync_now(app: &AppHandle, recheck: bool) {
    let Some(sync) = app.try_state::<WebDavSync>() else { return };
    if sync.store.lock().unwrap().is_none() {
        return;
    }
    let Some((key, mut copy)) = snapshot(app) else { return };

    let result = {
        let mut guard = sync.store.lock().unwrap();
        let Some(store) = guard.as_mut() else { return };
        if !vault::codec_has_key(store.codec(), &key) {
            // The master password changed here: re-upload under the new key
            match vault::remote_codec(&key) {
                Ok(codec) => store.set_codec(codec),
                Err(e) => {
                    drop(guard);
                    set_error(app, e);
                    return;
                }
            }
        }
        if recheck {
            store.recheck();
        }
        store.sync(&mut copy)
    };

    match result {
        Ok(outcome) => {
            if outcome.changed {
                merge_back(app, &key, copy);
            }
            set_health(app, Health { last_sync: Some(now_secs()), error: None, password_changed: false });
        }
        Err(StoreError::KeyMismatch) => {
            let last_sync = sync.health.lock().unwrap().last_sync;
            set_health(app, Health { last_sync, error: Some(describe(StoreError::KeyMismatch)), password_changed: true });
        }
        Err(e) => set_error(app, describe(e)),
    }
}

// ── Following the session ─────────────────────────────────────────────────────

/// After an unlock: connects to the active vault's server, if it has one,
/// and syncs straight away.
pub fn on_unlock(app: &AppHandle, key: &[u8; 32]) {
    let Some(sync) = app.try_state::<WebDavSync>() else { return };
    let Some(mut config) = app.state::<Vaults>().lock().unwrap().active().webdav.clone() else { return };

    // Typed when this vault was opened from the server; sealable only now
    if let Some(password) = sync.unsealed.lock().unwrap().take() {
        match seal(&password, key) {
            Ok(sealed) => {
                config.password = Some(sealed);
                if let Err(e) = save_config(app, Some(config.clone())) {
                    eprintln!("Could not save the WebDAV password: {e}");
                }
            }
            Err(e) => eprintln!("Could not seal the WebDAV password: {e}"),
        }
    }

    let Some(password) = config.password.as_deref().and_then(|s| unseal(s, key)) else {
        set_error(app, "Enter your WebDAV password again to keep syncing.".into());
        return;
    };
    let store = remote(&config, &password).and_then(|dav| Ok(RemoteStore::new(dav, vault::remote_codec(key)?)));
    match store {
        Ok(store) => {
            *sync.store.lock().unwrap() = Some(store);
            kick();
        }
        Err(e) => set_error(app, e),
    }
}

/// On lock and on switching vaults: forgets the server connection, which
/// holds a copy of the key, and anything half set up.
pub fn on_lock(app: &AppHandle) {
    if let Some(sync) = app.try_state::<WebDavSync>() {
        *sync.store.lock().unwrap() = None;
        *sync.join.lock().unwrap() = None;
    }
}

/// On switching vaults: also drops what belonged to the previous one.
pub fn reset(app: &AppHandle) {
    on_lock(app);
    if let Some(sync) = app.try_state::<WebDavSync>() {
        *sync.unsealed.lock().unwrap() = None;
        *sync.health.lock().unwrap() = Health::default();
    }
}

/// After a master password change here: seals the account password under
/// the new key. The next sync uploads the vault under it.
pub fn reseal(app: &AppHandle, old_key: &[u8; 32], new_key: &[u8; 32]) {
    let Some(mut config) = app.state::<Vaults>().lock().unwrap().active().webdav.clone() else { return };
    let Some(password) = config.password.as_deref().and_then(|s| unseal(s, old_key)) else { return };
    match seal(&password, new_key) {
        Ok(sealed) => {
            config.password = Some(sealed);
            if let Err(e) = save_config(app, Some(config)) {
                eprintln!("Could not save the WebDAV password: {e}");
            }
        }
        Err(e) => eprintln!("Could not seal the WebDAV password: {e}"),
    }
    kick();
}

// ── Setting it up ─────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
pub struct StatusView {
    configured: bool,
    url: Option<String>,
    username: Option<String>,
    /// Waiting for the master password of the vault already on the server
    awaiting_join: bool,
    #[serde(flatten)]
    health: Health,
}

pub fn status(app: &AppHandle) -> StatusView {
    let config = app.state::<Vaults>().lock().unwrap().active().webdav.clone();
    let sync = app.state::<WebDavSync>();
    let awaiting_join = sync.join.lock().unwrap().is_some();
    let health = sync.health.lock().unwrap().clone();
    StatusView {
        configured: config.is_some(),
        url: config.as_ref().map(|c| c.url.clone()),
        username: config.map(|c| c.username),
        awaiting_join,
        health,
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connect {
    /// Syncing: the server was empty and now has this vault, or already had it
    Connected,
    /// The server has another vault; `join` needs its master password
    NeedsPassword,
}

/// Starts syncing the open vault through the server folder at `url`.
pub fn connect(app: &AppHandle, url: &str, username: &str, password: Zeroizing<String>) -> Result<Connect, String> {
    let (key, mut copy) = snapshot(app).ok_or("Vault is locked")?;
    let url = normalize_url(url);
    check_server_free(app, &url)?;
    let mut config = WebDavConfig { url, username: username.trim().to_string(), password: None };
    let dav = remote(&config, &password)?;

    if let Fetched::Found { bytes, .. } = dav.get(None).map_err(describe)? {
        if !vault::opens_with(&bytes, &key)? {
            let sync = app.state::<WebDavSync>();
            *sync.join.lock().unwrap() = Some(PendingJoin { config, password, bytes });
            let _ = app.emit("webdav-status", ());
            return Ok(Connect::NeedsPassword);
        }
    }

    // An empty folder gets this vault; this same vault (set up before) syncs.
    // Done now, so a problem shows here rather than in the background.
    let mut store = RemoteStore::new(dav, vault::remote_codec(&key)?);
    let outcome = store.sync(&mut copy).map_err(describe)?;
    config.password = Some(seal(&password, &key)?);
    save_config(app, Some(config))?;
    *app.state::<WebDavSync>().store.lock().unwrap() = Some(store);
    if outcome.changed {
        merge_back(app, &key, copy);
    }
    set_health(app, Health { last_sync: Some(now_secs()), error: None, password_changed: false });
    Ok(Connect::Connected)
}

pub fn cancel_join(app: &AppHandle) {
    *app.state::<WebDavSync>().join.lock().unwrap() = None;
    let _ = app.emit("webdav-status", ());
}

/// Joins the vault on the server with its master password, merging this
/// device's entries into it: after `connect` found one there, or after its
/// password was changed on another device. Returns the new session key.
pub fn join(app: &AppHandle, master_password: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let sync = app.state::<WebDavSync>();
    let old_key = snapshot(app).ok_or("Vault is locked")?.0;

    // Either the server `connect` found, or the one this vault already uses
    let pending = match sync.join.lock().unwrap().take() {
        Some(p) => p,
        None => {
            let config = app.state::<Vaults>().lock().unwrap().active().webdav.clone().ok_or("This vault doesn't sync through a server")?;
            let password = config
                .password
                .as_deref()
                .and_then(|s| unseal(s, &old_key))
                .ok_or("Enter your WebDAV password again to keep syncing.")?;
            let bytes = match remote(&config, &password)?.get(None).map_err(describe)? {
                Fetched::Found { bytes, .. } => bytes,
                _ => return Err("There's no vault on the server any more".into()),
            };
            PendingJoin { config, password, bytes }
        }
    };

    let new_key = {
        let state = app.state::<VaultState>();
        let mut guard = state.lock().unwrap();
        let s = &mut *guard;
        let Some(data) = s.data.as_mut() else { return Err("Vault is locked".into()) };
        match vault::join_vault_bytes(&pending.bytes, master_password, data) {
            Ok(key) => {
                s.key = Some(key.clone());
                key
            }
            Err(e) => {
                // Keep it for another try at the password
                if sync.store.lock().unwrap().is_none() {
                    *sync.join.lock().unwrap() = Some(pending);
                }
                return Err(e);
            }
        }
    };

    let mut config = pending.config;
    config.password = Some(seal(&pending.password, &new_key)?);
    save_config(app, Some(config.clone()))?;
    let store = RemoteStore::new(remote(&config, &pending.password)?, vault::remote_codec(&new_key)?);
    *sync.store.lock().unwrap() = Some(store);
    set_health(app, Health::default());
    // Uploads the merged vault
    kick();
    let _ = app.emit("vault-changed", ());
    Ok(new_key)
}

/// Stops syncing the open vault. The copy on this device is the vault as
/// before; the server's copy stays for the other devices.
pub fn stop(app: &AppHandle) -> Result<(), String> {
    save_config(app, None)?;
    reset(app);
    let _ = app.emit("webdav-status", ());
    Ok(())
}

/// For a device with no copy of this vault yet: downloads the one another
/// device keeps on the server. The lock screen then asks for its master
/// password, and the first unlock starts syncing.
pub fn open_from_server(app: &AppHandle, url: &str, username: &str, password: Zeroizing<String>) -> Result<(), String> {
    if vault::vault_exists() {
        return Err("This vault already exists. Connect it to the server from Settings.".into());
    }
    let url = normalize_url(url);
    check_server_free(app, &url)?;
    let config = WebDavConfig { url, username: username.trim().to_string(), password: None };
    let bytes = match remote(&config, &password)?.get(None).map_err(describe)? {
        Fetched::Found { bytes, .. } => bytes,
        _ => {
            return Err("There's no vault in that folder on the server. Check the address, or set up sync from a device that has your vault.".into())
        }
    };
    vault::adopt_vault_file(&bytes)?;
    if let Err(e) = save_config(app, Some(config)) {
        // Without the setting, the downloaded copy would never sync
        let _ = std::fs::remove_file(vault::vault_dir()?.join(FILE_NAME));
        return Err(e);
    }
    *app.state::<WebDavSync>().unsealed.lock().unwrap() = Some(password);
    Ok(())
}
