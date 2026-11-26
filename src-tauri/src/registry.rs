// The vaults this device knows about, for people who share it.
//
// Each vault is a separate vault.enc with its own master password, its own
// folder on this device and, optionally, its own sync folder or WebDAV
// server (one or the other, never both). Exactly one is
// active: that is the one the lock screen unlocks and every other command
// works on. Only one is ever open, so switching locks the one that was.
//
// The list lives in vaults.json in the app's data folder. It holds names,
// paths and server addresses, and never leaves this device. The one secret,
// a WebDAV account password, is sealed under its vault's key (webdav.rs).

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const REGISTRY_FILE: &str = "vaults.json";
/// The sync folder setting from before there was more than one vault
pub const LEGACY_SYNC_FILE: &str = "sync_folder.conf";
/// New vaults get a folder each under this, in the app's data folder
const VAULTS_SUBDIR: &str = "vaults";
const MAX_NAME_LEN: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VaultRecord {
    pub id: String,
    pub name: String,
    /// Where the vault lives on this device when it isn't in a sync folder
    pub home: PathBuf,
    #[serde(default)]
    pub sync_folder: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webdav: Option<WebDavConfig>,
}

/// A WebDAV server a vault syncs through.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WebDavConfig {
    /// The folder on the server that holds vault.enc
    pub url: String,
    pub username: String,
    /// The account password, sealed under the vault's key (base64 of nonce
    /// and AES-GCM ciphertext). None from opening the vault on this device
    /// until its first unlock, when the key to seal it with is there.
    #[serde(default)]
    pub password: Option<String>,
}

impl WebDavConfig {
    /// The folder's address in a form to compare: two vaults on one server
    /// folder would merge into each other.
    fn folder_key(&self) -> String {
        normalize_url(&self.url).to_ascii_lowercase()
    }
}

/// A server folder's address without surrounding space or trailing slashes.
pub fn normalize_url(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

impl VaultRecord {
    /// The folder vault.enc is in right now.
    pub fn dir(&self) -> &Path {
        self.sync_folder.as_deref().unwrap_or(&self.home)
    }

    pub fn exists(&self) -> bool {
        self.dir().join("vault.enc").exists()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Registry {
    pub active: String,
    pub vaults: Vec<VaultRecord>,
    /// The vault iOS AutoFill shows. The extension holds one snapshot, so
    /// only one vault can feed it.
    #[serde(default)]
    pub autofill: Option<String>,
    /// The app's data folder; vaults.json and new vaults' folders go here
    #[serde(skip)]
    data_dir: PathBuf,
}

impl Registry {
    /// Reads the list, or builds it on first run from the single vault every
    /// earlier version kept at `legacy_home`. A list that can't be read is set
    /// aside rather than overwritten, and rebuilt from the vault folders found.
    pub fn load(data_dir: &Path, legacy_home: PathBuf) -> Registry {
        let file = data_dir.join(REGISTRY_FILE);
        if let Ok(text) = fs::read_to_string(&file) {
            match serde_json::from_str::<Registry>(&text) {
                Ok(mut reg) if !reg.vaults.is_empty() => {
                    reg.data_dir = data_dir.to_path_buf();
                    if reg.get(&reg.active).is_none() {
                        reg.active = reg.vaults[0].id.clone();
                    }
                    return reg;
                }
                _ => {
                    eprintln!("{} is unreadable; rebuilding the vault list", file.display());
                    let _ = fs::rename(&file, file.with_extension("json.bad"));
                }
            }
        }

        let legacy_sync = fs::read_to_string(data_dir.join(LEGACY_SYNC_FILE))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from);
        let first = VaultRecord {
            id: uuid::Uuid::new_v4().to_string(),
            name: "My vault".into(),
            home: legacy_home,
            sync_folder: legacy_sync,
            webdav: None,
        };
        let mut reg = Registry {
            active: first.id.clone(),
            vaults: vec![first],
            autofill: None,
            data_dir: data_dir.to_path_buf(),
        };
        reg.recover_orphans();
        if let Err(e) = reg.save() {
            eprintln!("Could not save the vault list: {e}");
        }
        reg
    }

    /// Lists vaults whose folders exist under vaults/ but which the list
    /// doesn't name, so a lost list doesn't hide them.
    fn recover_orphans(&mut self) {
        let Ok(dirs) = fs::read_dir(self.data_dir.join(VAULTS_SUBDIR)) else {
            return;
        };
        for dir in dirs.flatten() {
            let home = dir.path();
            let known = self.vaults.iter().any(|v| v.home == home);
            if known || !home.join("vault.enc").exists() {
                continue;
            }
            let name = self.unused_name("Recovered vault");
            self.vaults.push(VaultRecord {
                id: dir.file_name().to_string_lossy().into_owned(),
                name,
                home,
                sync_folder: None,
                webdav: None,
            });
        }
    }

    fn unused_name(&self, base: &str) -> String {
        (1..)
            .map(|n| if n == 1 { base.to_string() } else { format!("{base} {n}") })
            .find(|n| !self.vaults.iter().any(|v| v.name.eq_ignore_ascii_case(n)))
            .unwrap()
    }

    /// Written atomically, so a crash can't leave a half-written list.
    pub fn save(&self) -> Result<(), String> {
        fs::create_dir_all(&self.data_dir).map_err(|e| e.to_string())?;
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        folder_crdt::fs::write_atomic(&self.data_dir.join(REGISTRY_FILE), &json)
            .map_err(|e| e.to_string())
    }

    pub fn get(&self, id: &str) -> Option<&VaultRecord> {
        self.vaults.iter().find(|v| v.id == id)
    }

    fn get_mut(&mut self, id: &str) -> Result<&mut VaultRecord, String> {
        self.vaults.iter_mut().find(|v| v.id == id).ok_or_else(|| "No such vault".into())
    }

    pub fn active(&self) -> &VaultRecord {
        // `load` and `remove` keep `active` pointing at a listed vault
        self.get(&self.active).unwrap_or(&self.vaults[0])
    }

    fn check_name(&self, name: &str, except: Option<&str>) -> Result<String, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the vault a name".into());
        }
        if name.chars().count() > MAX_NAME_LEN {
            return Err(format!("Keep the name under {MAX_NAME_LEN} characters"));
        }
        let taken = self
            .vaults
            .iter()
            .any(|v| Some(v.id.as_str()) != except && v.name.eq_ignore_ascii_case(name));
        if taken {
            return Err("There's already a vault with that name".into());
        }
        Ok(name.to_string())
    }

    /// Lists a new, empty vault with a folder of its own. Nothing is created
    /// on disk until the user sets its master password.
    pub fn add(&mut self, name: &str) -> Result<String, String> {
        let name = self.check_name(name, None)?;
        let id = uuid::Uuid::new_v4().to_string();
        let home = self.data_dir.join(VAULTS_SUBDIR).join(&id);
        self.vaults.push(VaultRecord { id: id.clone(), name, home, sync_folder: None, webdav: None });
        Ok(id)
    }

    pub fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = self.check_name(name, Some(id))?;
        self.get_mut(id)?.name = name;
        Ok(())
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        self.get_mut(id)?;
        self.active = id.to_string();
        Ok(())
    }

    pub fn set_sync_folder(&mut self, id: &str, folder: Option<PathBuf>) -> Result<(), String> {
        self.get_mut(id)?.sync_folder = folder;
        Ok(())
    }

    pub fn set_webdav(&mut self, id: &str, webdav: Option<WebDavConfig>) -> Result<(), String> {
        self.get_mut(id)?.webdav = webdav;
        Ok(())
    }

    /// The vault other than `except` that already syncs through the server
    /// folder at `url`.
    pub fn using_server(&self, url: &str, except: &str) -> Option<&VaultRecord> {
        let want = normalize_url(url).to_ascii_lowercase();
        self.vaults
            .iter()
            .find(|v| v.id != except && v.webdav.as_ref().is_some_and(|w| w.folder_key() == want))
    }

    /// The vault other than `except` that already keeps its file in `dir`.
    /// Two vaults in one folder would read each other's file and conflict
    /// copies as their own.
    pub fn using_folder(&self, dir: &Path, except: &str) -> Option<&VaultRecord> {
        self.vaults.iter().find(|v| v.id != except && same_dir(v.dir(), dir))
    }

    /// Drops `id` from the list and, if it was active, makes another vault
    /// active. Returns the record so the caller can clear up its files.
    pub fn remove(&mut self, id: &str) -> Result<VaultRecord, String> {
        if self.vaults.len() == 1 {
            return Err("This is the only vault on this device".into());
        }
        let at = self.vaults.iter().position(|v| v.id == id).ok_or("No such vault")?;
        let removed = self.vaults.remove(at);
        if self.active == id {
            self.active = self.vaults[0].id.clone();
        }
        if self.autofill.as_deref() == Some(id) {
            self.autofill = None;
        }
        Ok(removed)
    }

    /// Whether `home` is a folder Vault made for one vault, and so can be
    /// deleted along with it. The original vault's folder predates this.
    pub fn owns_home(&self, home: &Path) -> bool {
        home.parent() == Some(&self.data_dir.join(VAULTS_SUBDIR))
    }
}

/// Compares folders the way the filesystem does, falling back to the paths
/// as given when one doesn't exist yet.
fn same_dir(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("vault_registry_tests").join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn first_run_lists_the_existing_vault_and_its_sync_folder() {
        let data = temp_dir("first_run");
        let legacy = data.join("legacy");
        fs::write(data.join(LEGACY_SYNC_FILE), "D:\\OneDrive\\Vault\n").unwrap();

        let reg = Registry::load(&data, legacy.clone());
        assert_eq!(reg.vaults.len(), 1);
        assert_eq!(reg.active().home, legacy);
        assert_eq!(reg.active().sync_folder, Some(PathBuf::from("D:\\OneDrive\\Vault")));
        assert_eq!(reg.active().dir(), Path::new("D:\\OneDrive\\Vault"));

        // And it is remembered, ids included
        let again = Registry::load(&data, PathBuf::from("ignored"));
        assert_eq!(again.vaults, reg.vaults);
        assert_eq!(again.active, reg.active);
    }

    #[test]
    fn added_vaults_get_their_own_folder_and_unique_names() {
        let data = temp_dir("add");
        let mut reg = Registry::load(&data, data.join("legacy"));

        let id = reg.add("  Work  ").unwrap();
        let work = reg.get(&id).unwrap();
        assert_eq!(work.name, "Work");
        assert_eq!(work.home, data.join("vaults").join(&id));
        assert!(reg.owns_home(&work.home));
        assert!(!reg.owns_home(&data.join("legacy")));

        assert!(reg.add("work").is_err(), "names are unique, ignoring case");
        assert!(reg.add("   ").is_err());
        assert!(reg.add(&"x".repeat(41)).is_err());
        reg.rename(&id, "WORK").unwrap();
        assert!(reg.rename(&id, "My vault").is_err());
    }

    #[test]
    fn removing_the_active_vault_activates_another() {
        let data = temp_dir("remove");
        let mut reg = Registry::load(&data, data.join("legacy"));
        let first = reg.active.clone();
        let second = reg.add("Second").unwrap();
        reg.set_active(&second).unwrap();
        reg.autofill = Some(second.clone());

        reg.remove(&second).unwrap();
        assert_eq!(reg.active, first);
        assert_eq!(reg.autofill, None);
        assert!(reg.remove(&first).is_err(), "the last vault stays");
        assert!(reg.set_active("nope").is_err());
    }

    #[test]
    fn a_sync_folder_belongs_to_one_vault() {
        let data = temp_dir("sync_folder");
        let shared = data.join("shared");
        fs::create_dir_all(&shared).unwrap();
        let mut reg = Registry::load(&data, data.join("legacy"));
        let first = reg.active.clone();
        let second = reg.add("Second").unwrap();

        reg.set_sync_folder(&first, Some(shared.clone())).unwrap();
        assert_eq!(reg.using_folder(&shared, &second).map(|v| v.id.as_str()), Some(first.as_str()));
        assert!(reg.using_folder(&shared, &first).is_none());
        assert!(reg.using_folder(&data.join("elsewhere"), &second).is_none());
    }

    #[test]
    fn a_server_folder_belongs_to_one_vault_and_is_remembered() {
        let data = temp_dir("webdav");
        let mut reg = Registry::load(&data, data.join("legacy"));
        let first = reg.active.clone();
        let second = reg.add("Second").unwrap();
        let config = WebDavConfig {
            url: "https://dav.example.com/Vault/".into(),
            username: "me".into(),
            password: Some("sealed".into()),
        };
        reg.set_webdav(&first, Some(config.clone())).unwrap();
        reg.save().unwrap();

        // Same folder however it's written
        assert_eq!(reg.using_server(" HTTPS://dav.example.com/Vault ", &second).map(|v| v.id.as_str()), Some(first.as_str()));
        assert!(reg.using_server("https://dav.example.com/Vault", &first).is_none());
        assert!(reg.using_server("https://dav.example.com/Other", &second).is_none());

        let again = Registry::load(&data, data.join("legacy"));
        assert_eq!(again.get(&first).unwrap().webdav, Some(config));
        assert_eq!(again.get(&second).unwrap().webdav, None);
    }

    #[test]
    fn an_unreadable_list_is_kept_aside_and_vault_folders_are_recovered() {
        let data = temp_dir("corrupt");
        let orphan = data.join("vaults").join("abc");
        fs::create_dir_all(&orphan).unwrap();
        fs::write(orphan.join("vault.enc"), "{}").unwrap();
        fs::write(data.join(REGISTRY_FILE), "not json").unwrap();

        let reg = Registry::load(&data, data.join("legacy"));
        assert!(data.join("vaults.json.bad").exists());
        let recovered = reg.get("abc").unwrap();
        assert_eq!(recovered.name, "Recovered vault");
        assert_eq!(recovered.home, orphan);
        assert_eq!(reg.vaults.len(), 2);
    }
}
