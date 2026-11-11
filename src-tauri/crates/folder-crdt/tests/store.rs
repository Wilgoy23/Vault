//! FolderStore in a temp directory, with devices simulated as separate
//! stores on one folder and sync clients simulated by the files they leave.

use std::fs;
use std::path::{Path, PathBuf};

use folder_crdt::codec::Envelope;
use folder_crdt::format::FormatError;
use folder_crdt::{
    Cipher, DeleteWins, EditRevives, FolderStore, Hlc, Json, KeyMismatch, Lww, Merge, Record, RecordSet, Sealed,
    Store, StoreError, Unknown,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use zeroize::Zeroizing;

// ── A small app with two collections ──────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Item {
    title: Lww<String, Hlc>,
    #[serde(flatten)]
    extra: Unknown,
}

impl Merge for Item {
    fn merge(&mut self, other: Self) {
        self.title.merge(other.title.clone());
        self.extra.merge(other.extra.clone());
    }
}

impl Record for Item {
    type Clock = Hlc;
    fn last_edit(&self) -> Option<Hlc> {
        self.title.clock().cloned()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Todo {
    lists: RecordSet<String, Item, DeleteWins>,
    tags: RecordSet<String, Item, EditRevives>,
    #[serde(flatten)]
    extra: Unknown,
}

impl Merge for Todo {
    fn merge(&mut self, mut other: Self) {
        self.lists.merge(std::mem::take(&mut other.lists));
        self.tags.merge(std::mem::take(&mut other.tags));
        self.extra.merge(std::mem::take(&mut other.extra));
    }
}

fn item(title: &str, ms: u64, device: u64) -> Item {
    Item { title: Lww::new(title.into(), Hlc::new(ms, 0, device)), extra: Unknown::default() }
}

fn titles(t: &Todo) -> Vec<String> {
    let mut v: Vec<String> = t.lists.iter().map(|(_, i)| i.title.get().unwrap().clone()).collect();
    v.sort();
    v
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("folder_crdt_store_tests").join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn plain(dir: &Path) -> FolderStore<Json<Todo>> {
    FolderStore::new(dir, "todo", "json", Json::new("todo", 1))
}

/// Writes `data` where a sync client would leave a conflict copy.
fn conflict_copy(dir: &Path, name: &str, data: &Todo) -> PathBuf {
    let path = dir.join(name);
    let mut other = FolderStore::new(dir, "scratch", "tmp", Json::<Todo>::new("todo", 1));
    other.save(&mut data.clone()).unwrap();
    fs::rename(other.path(), &path).unwrap();
    path
}

// ── A stand-in cipher ────────────────────────────────────────────────────

/// Not encryption: a keyed XOR with a check byte. Enough to tell keys apart.
struct Xor(u8);

impl Cipher for Xor {
    fn params(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert("salt".into(), "s".into());
        m
    }
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
        Ok(std::iter::once(self.0).chain(plaintext.iter().map(|b| b ^ self.0)).collect())
    }
    fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyMismatch> {
        match sealed.split_first() {
            Some((&k, rest)) if k == self.0 => Ok(Zeroizing::new(rest.iter().map(|b| b ^ self.0).collect())),
            _ => Err(KeyMismatch),
        }
    }
}

fn sealed(dir: &Path, key: u8) -> FolderStore<Sealed<Todo, Xor>> {
    FolderStore::new(dir, "todo", "enc", Sealed::new("todo", 1, Xor(key)))
}

// ── Tests ────────────────────────────────────────────────────────────────

#[test]
fn works_without_a_cipher_and_the_file_is_readable_json() {
    let dir = temp_dir("plain");
    let mut store = plain(&dir);
    let mut data = Todo::default();
    data.lists.upsert("a".into(), item("Groceries", 1, 1));
    data.tags.upsert("t".into(), item("home", 1, 1));
    store.create(&data).unwrap();
    assert!(store.create(&data).is_err(), "create never overwrites");

    let file: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(file["app"], "todo");
    assert_eq!(file["data"]["lists"]["records"]["a"]["title"][1], "Groceries");
    assert_eq!(plain(&dir).load().unwrap(), data);
}

#[test]
fn merges_each_sync_clients_conflict_copies_and_deletes_them_after() {
    let dir = temp_dir("copies");
    let mut a = plain(&dir);
    let mut data = Todo::default();
    data.lists.upsert("a".into(), item("From A", 1, 1));
    a.create(&data).unwrap();

    let names = [
        ("todo (Sam's conflicted copy).json", "Dropbox"),
        ("todo-LAPTOP.json", "OneDrive"),
        ("todo.sync-conflict-20261110-120000-ABCDEFG.json", "Syncthing"),
        ("todo 2.json", "iCloud"),
    ];
    let mut copies = Vec::new();
    for (i, (name, client)) in names.iter().enumerate() {
        let mut theirs = Todo::default();
        theirs.lists.upsert(client.to_string(), item(client, 2, 10 + i as u64));
        copies.push(conflict_copy(&dir, name, &theirs));
    }
    let backup = dir.join("todo-backup.json");
    fs::write(&backup, "an export").unwrap();

    let outcome = a.sync(&mut data).unwrap();
    assert!(outcome.changed && outcome.wrote);
    assert_eq!(outcome.merged.len(), 4);
    assert!(outcome.unreadable.is_empty(), "{:?}", outcome.unreadable);
    assert_eq!(titles(&data), ["Dropbox", "From A", "OneDrive", "Syncthing", "iCloud"]);
    assert!(copies.iter().all(|c| !c.exists()));
    assert!(backup.exists());
    assert_eq!(plain(&dir).load().unwrap(), data);

    let again = a.sync(&mut data).unwrap();
    assert!(!again.changed && !again.wrote);
}

#[test]
fn each_store_remembers_what_it_last_saw() {
    let dir = temp_dir("last_seen");
    let mut a = plain(&dir);
    let mut a_data = Todo::default();
    a_data.lists.upsert("a".into(), item("From A", 1, 1));
    a.create(&a_data).unwrap();

    // Device B opens the same file and saves an edit
    let mut b = plain(&dir);
    let mut b_data = b.load().unwrap();
    b_data.lists.upsert("b".into(), item("From B", 2, 2));
    b.save(&mut b_data).unwrap();

    // A saves without having synced: B's edit is merged in, not overwritten
    a_data.lists.upsert("a2".into(), item("Also A", 3, 1));
    a.save(&mut a_data).unwrap();
    assert_eq!(titles(&a_data), ["Also A", "From A", "From B"]);

    // B syncs and sees A's; nothing left to write after that
    let outcome = b.sync(&mut b_data).unwrap();
    assert!(outcome.changed && !outcome.wrote);
    assert_eq!(b_data, a_data);
}

#[test]
fn a_file_under_another_key_is_never_overwritten() {
    let dir = temp_dir("key_mismatch");
    let mut a = sealed(&dir, 1);
    let mut data = Todo::default();
    a.create(&data).unwrap();
    assert!(!String::from_utf8_lossy(&fs::read(a.path()).unwrap()).contains("records"));

    // Another device changes the key
    let mut b = sealed(&dir, 1);
    let mut b_data = b.load().unwrap();
    b.set_codec(Sealed::new("todo", 1, Xor(2)));
    b.save(&mut b_data).unwrap();
    let rekeyed = fs::read(a.path()).unwrap();

    data.lists.upsert("late".into(), item("Late", 5, 1));
    assert!(matches!(a.save(&mut data), Err(StoreError::KeyMismatch)));
    assert!(matches!(a.sync(&mut data), Err(StoreError::KeyMismatch)));
    assert_eq!(fs::read(a.path()).unwrap(), rekeyed);
}

#[test]
fn unreadable_copies_are_left_in_place_and_reported() {
    let dir = temp_dir("unreadable");
    let mut a = sealed(&dir, 1);
    let mut data = Todo::default();
    a.create(&data).unwrap();

    let mut other_key = FolderStore::new(&dir, "elsewhere", "enc", Sealed::new("todo", 1, Xor(9)));
    let mut theirs = Todo::default();
    theirs.lists.upsert("x".into(), item("Other key", 1, 3));
    other_key.save(&mut theirs).unwrap();
    let wrong_key = dir.join("todo 2.enc");
    fs::rename(other_key.path(), &wrong_key).unwrap();

    let newer = dir.join("todo 3.enc");
    let mut file: Value = serde_json::from_slice(&fs::read(a.path()).unwrap()).unwrap();
    file["schema"] = 2.into();
    fs::write(&newer, file.to_string()).unwrap();
    let damaged = dir.join("todo 4.enc");
    fs::write(&damaged, "{\"magic\": \"folder-crdt\"").unwrap();

    let outcome = a.sync(&mut data).unwrap();
    assert!(!outcome.changed && !outcome.wrote);
    let mut reported: Vec<(String, &StoreError)> = outcome
        .unreadable
        .iter()
        .map(|(p, e)| (p.file_name().unwrap().to_string_lossy().into_owned(), e))
        .collect();
    reported.sort_by(|x, y| x.0.cmp(&y.0));
    assert!(matches!(reported[0], (ref n, StoreError::KeyMismatch) if n == "todo 2.enc"));
    assert!(matches!(reported[1], (ref n, StoreError::Format(FormatError::Newer { .. }))if n == "todo 3.enc"));
    assert!(matches!(reported[2], (ref n, StoreError::Corrupt(_)) if n == "todo 4.enc"));
    assert!(wrong_key.exists() && newer.exists() && damaged.exists());
}

#[test]
fn collections_and_fields_from_a_newer_build_survive() {
    let dir = temp_dir("unknown");
    let mut store = plain(&dir);
    let mut data = Todo::default();
    data.lists.upsert("a".into(), item("Groceries", 1, 1));
    store.create(&data).unwrap();

    // A newer build adds a collection and a field to each list
    let mut file: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    file["data"]["reminders"] = json!({ "records": { "r": { "at": [[5, 0, 2], 99] } } });
    file["data"]["lists"]["records"]["a"]["colour"] = json!([[5, 0, 2], "green"]);
    fs::write(store.path(), file.to_string()).unwrap();

    // This build syncs, edits and saves
    store.sync(&mut data).unwrap();
    data.lists.upsert("b".into(), item("Chores", 6, 1));
    store.save(&mut data).unwrap();

    let saved: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(saved["data"]["reminders"], file["data"]["reminders"]);
    assert_eq!(saved["data"]["lists"]["records"]["a"]["colour"], json!([[5, 0, 2], "green"]));
}

#[test]
fn a_missing_file_is_left_for_the_next_sync_and_rewritten_by_a_save() {
    let dir = temp_dir("missing");
    let mut store = plain(&dir);
    let mut data = Todo::default();
    data.lists.upsert("a".into(), item("Kept", 1, 1));
    store.create(&data).unwrap();
    fs::remove_file(store.path()).unwrap();

    let outcome = store.sync(&mut data).unwrap();
    assert!(!outcome.wrote && !store.exists());
    assert!(matches!(store.load(), Err(StoreError::Io(_))));
    store.save(&mut data).unwrap();
    assert_eq!(plain(&dir).load().unwrap(), data);
}

#[test]
fn oversized_files_are_refused() {
    let dir = temp_dir("oversized");
    let mut store = plain(&dir).max_len(64);
    fs::write(store.path(), vec![b' '; 65]).unwrap();
    assert!(matches!(store.load(), Err(StoreError::Corrupt(_))));
}

#[test]
fn extra_skip_prefixes_are_honoured() {
    let dir = temp_dir("skip");
    let mut store = plain(&dir).skip("todo-archive");
    let mut data = Todo::default();
    store.create(&data).unwrap();
    fs::write(dir.join("todo-archive-2025.json"), "not merged").unwrap();
    assert!(store.conflict_copies().is_empty());
    assert!(store.sync(&mut data).unwrap().unreadable.is_empty());
}

#[test]
fn the_envelope_exposes_params_without_opening_the_payload() {
    let dir = temp_dir("params");
    let mut store = sealed(&dir, 3);
    store.create(&Todo::default()).unwrap();
    let env = Envelope::parse(&fs::read(store.path()).unwrap()).unwrap();
    assert_eq!(env.params["salt"], "s");
    assert_eq!(env.header.unwrap().app, "todo");
}
