//! Vault's records: the behaviours people rely on, migration from schema 1,
//! and fields from newer builds surviving.

use folder_crdt::vault::legacy;
use folder_crdt::vault::*;
use folder_crdt::{merged, HlcClock, Merge};
use serde_json::json;

/// A device with its own clock, editing its own copy.
struct Device {
    clock: HlcClock,
    secs: u64,
    data: VaultData,
}

impl Device {
    fn new(id: u64, data: &VaultData) -> Self {
        let mut clock = HlcClock::new(id);
        if let Some(seen) = data.latest_clock() {
            clock.observe(seen);
        }
        Device { clock, secs: 1_700_000_000, data: data.clone() }
    }

    /// The time `dt` seconds after this device's last edit.
    fn now(&mut self, dt: u64) -> folder_crdt::Hlc {
        self.secs += dt;
        self.clock.tick_at(self.secs * 1000)
    }

    fn add(&mut self, id: &str, name: &str, password: &str) {
        let now = self.now(1);
        self.data.add_entry(id.into(), fields(name, password), now).unwrap();
    }

    fn edit(&mut self, id: &str, change: impl FnOnce(&mut Fields)) {
        let mut f = form(&self.data.entry_view(id).unwrap());
        change(&mut f);
        let now = self.now(1);
        self.data.update_entry(id, f, now).unwrap();
    }
}

fn fields(name: &str, password: &str) -> Fields {
    Fields {
        name: name.into(),
        username: None,
        email: "user@example.com".into(),
        password: password.into(),
        url: None,
        notes: None,
        folder_id: None,
        totp_secret: None,
    }
}

fn form(e: &EntryView) -> Fields {
    Fields {
        name: e.name.clone(),
        username: e.username.clone(),
        email: e.email.clone(),
        password: e.password.clone(),
        url: e.url.clone(),
        notes: e.notes.clone(),
        folder_id: e.folder_id.clone(),
        totp_secret: e.totp_secret.clone(),
    }
}

/// Both merge orders, checked to agree.
fn both_ways(a: &VaultData, b: &VaultData) -> VaultData {
    let ab = merged(a.clone(), b.clone());
    let ba = merged(b.clone(), a.clone());
    assert_eq!(*canonical(&ab), *canonical(&ba), "merge order changed the result");
    ab
}

fn one(data: &VaultData, id: &str) -> EntryView {
    data.entry_view(id).unwrap()
}

#[test]
fn edits_on_two_devices_to_different_entries_both_survive() {
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "pw");
    let mut laptop = Device::new(2, &base.data);
    laptop.add("2", "Only on laptop", "pw");
    laptop.edit("1", |f| f.name = "New name".into());
    let mut phone = Device::new(3, &base.data);
    phone.add("3", "Only on phone", "pw");

    let merged = both_ways(&laptop.data, &phone.data);
    let names: Vec<String> = merged.entry_views().into_iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, ["New name", "Only on laptop", "Only on phone"]);
}

#[test]
fn edits_to_different_fields_of_one_entry_both_survive() {
    // Schema 1 kept whichever copy of the entry was edited last
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "pw");
    let mut laptop = Device::new(2, &base.data);
    laptop.edit("1", |f| f.notes = Some("from laptop".into()));
    let mut phone = Device::new(3, &base.data);
    phone.secs += 60;
    phone.edit("1", |f| f.url = Some("https://example.com".into()));

    let e = one(&both_ways(&laptop.data, &phone.data), "1");
    assert_eq!(e.notes.as_deref(), Some("from laptop"));
    assert_eq!(e.url.as_deref(), Some("https://example.com"));
}

#[test]
fn a_rotation_survives_a_later_edit_on_another_device() {
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "old-pw");
    let mut a = Device::new(2, &base.data);
    a.edit("1", |f| f.password = "new-pw".into());
    let mut b = Device::new(3, &base.data);
    b.secs += 100;
    b.edit("1", |f| f.notes = Some("edited on B".into()));

    let e = one(&both_ways(&a.data, &b.data), "1");
    assert_eq!(e.password, "new-pw");
    assert_eq!(e.notes.as_deref(), Some("edited on B"));
    let history: Vec<&str> = e.password_history.iter().map(|p| p.password.as_str()).collect();
    assert_eq!(history, ["old-pw"]);
}

#[test]
fn two_rotations_in_the_same_second_keep_the_loser_in_history_with_its_own_time() {
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "original");
    let mut a = Device::new(2, &base.data);
    let mut b = Device::new(3, &base.data);
    a.edit("1", |f| f.password = "from-a".into());
    b.edit("1", |f| f.password = "from-b".into());
    assert_eq!(a.secs, b.secs);

    let e = one(&both_ways(&a.data, &b.data), "1");
    // Device 3's clock wins the tie, whatever the passwords say
    assert_eq!(e.password, "from-b");
    let history: Vec<&str> = e.password_history.iter().map(|p| p.password.as_str()).collect();
    assert_eq!(history, ["original", "from-a"]);
    assert!(!history.contains(&"from-b"));
}

#[test]
fn history_keeps_the_last_ten_passwords() {
    let mut d = Device::new(1, &VaultData::default());
    d.add("1", "Site", "pw0");
    for i in 1..=15 {
        d.edit("1", |f| f.password = format!("pw{i}"));
    }
    let e = one(&d.data, "1");
    assert_eq!(e.password, "pw15");
    let history: Vec<String> = e.password_history.iter().map(|p| p.password.clone()).collect();
    let expected: Vec<String> = (5..15).map(|i| format!("pw{i}")).collect();
    assert_eq!(history, expected);
    // Each was replaced when the next was set
    assert!(e.password_history.windows(2).all(|w| w[0].replaced_at < w[1].replaced_at));
}

#[test]
fn saving_an_unchanged_form_writes_nothing() {
    let mut d = Device::new(1, &VaultData::default());
    d.add("1", "Site", "pw");
    let before = canonical(&d.data);
    d.edit("1", |_| {});
    assert_eq!(*canonical(&d.data), *before);
}

#[test]
fn a_delete_is_final_and_takes_the_passwords_with_it() {
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "hunter2");
    let mut deleter = Device::new(2, &base.data);
    let now = deleter.now(1);
    deleter.data.delete_entry("1", now).unwrap();
    let mut editor = Device::new(3, &base.data);
    editor.secs += 100;
    editor.edit("1", |f| f.password = "after-the-delete".into());

    let merged = both_ways(&deleter.data, &editor.data);
    assert!(merged.entry_views().is_empty());
    let on_disk = String::from_utf8(canonical(&merged).to_vec()).unwrap();
    assert!(!on_disk.contains("hunter2") && !on_disk.contains("after-the-delete"));
}

#[test]
fn a_folder_deleted_elsewhere_unfiles_its_entries() {
    let mut base = Device::new(1, &VaultData::default());
    let now = base.now(1);
    base.data.add_folder("f".into(), "Work".into(), now).unwrap();
    base.add("1", "Filed", "pw");
    base.edit("1", |f| f.folder_id = Some("f".into()));

    let mut a = Device::new(2, &base.data);
    let now = a.now(1);
    a.data.delete_folder("f", now).unwrap();
    // B files a new entry under the folder, not knowing it's gone
    let mut b = Device::new(3, &base.data);
    b.add("2", "Also filed", "pw");
    b.edit("2", |f| f.folder_id = Some("f".into()));

    let merged = both_ways(&a.data, &b.data);
    assert!(merged.folder_views().is_empty());
    assert!(merged.entry_views().iter().all(|e| e.folder_id.is_none()));
}

#[test]
fn uses_on_two_devices_add_up() {
    let mut base = Device::new(1, &VaultData::default());
    base.add("1", "Site", "pw");
    let mut a = Device::new(2, &base.data);
    let mut b = Device::new(3, &base.data);
    for _ in 0..2 {
        let now = a.now(1);
        a.data.mark_entry_used("1", now).unwrap();
    }
    for _ in 0..3 {
        let now = b.now(5);
        b.data.mark_entry_used("1", now).unwrap();
    }
    let e = one(&both_ways(&a.data, &b.data), "1");
    assert_eq!(e.use_count, 5);
    assert_eq!(e.last_used_at, Some(b.secs));
}

// ── Migration ─────────────────────────────────────────────────────────────────

/// A schema 1 vault with history, folders, uses and both kinds of tombstone.
fn schema_1() -> serde_json::Value {
    json!({
        "entries": [
            {
                "id": "b", "name": "Bank", "kind": "login", "email": "me@example.com",
                "password": "pw3", "url": "https://bank.example", "notes": null,
                "folder_id": "f1", "created_at": 100, "updated_at": 400,
                "password_changed_at": 300,
                "password_history": [
                    { "password": "pw1", "replaced_at": 200 },
                    { "password": "pw2", "replaced_at": 300 }
                ],
                "last_used_at": 500, "use_count": 7
            },
            {
                "id": "a", "name": "Mail", "username": "me", "email": "",
                "password": "secret", "url": null, "notes": "n",
                "created_at": 100, "updated_at": 100
            },
            {
                "id": "revived", "name": "Edited after its delete", "email": "",
                "password": "x", "url": null, "notes": null,
                "created_at": 50, "updated_at": 900
            }
        ],
        "folders": [
            { "id": "f2", "name": "Second", "updated_at": 10 },
            { "id": "f1", "name": "First", "updated_at": 20 }
        ],
        "deleted": [
            { "id": "gone", "deleted_at": 600 },
            { "id": "revived", "deleted_at": 800 }
        ]
    })
}

fn migrated(v1: serde_json::Value) -> VaultData {
    let old: legacy::VaultData = serde_json::from_value(v1).unwrap();
    legacy::migrate(&old)
}

#[test]
fn a_schema_1_vault_looks_the_same_after_migration() {
    let v1 = schema_1();
    let data = migrated(v1.clone());

    // Same entries with the same values, now in creation order. Schema 1
    // kept file order, which differed between devices (merges appended).
    let shown = serde_json::to_value(data.entry_views()).unwrap();
    let mut expected = v1["entries"].clone();
    let revived = expected.as_array_mut().unwrap().remove(2);
    expected.as_array_mut().unwrap().insert(0, revived);
    // Entries saved before rotation dates were tracked now show their last edit
    expected[0]["password_changed_at"] = json!(900);
    expected[2]["password_changed_at"] = json!(100);
    for e in expected.as_array_mut().unwrap() {
        let e = e.as_object_mut().unwrap();
        e.entry("kind").or_insert(json!("login"));
        e.entry("use_count").or_insert(json!(0));
    }
    assert_eq!(shown, expected);

    let folders = serde_json::to_value(data.folder_views()).unwrap();
    assert_eq!(folders, v1["folders"]);

    assert!(data.entries.is_deleted("gone"));
    assert!(data.entry_view("revived").is_some(), "an entry its tombstone didn't beat stays");
}

#[test]
fn two_devices_migrating_the_same_file_agree() {
    let a = migrated(schema_1());
    let b = migrated(schema_1());
    assert_eq!(*canonical(&a), *canonical(&b));
    assert_eq!(*canonical(&merged(a.clone(), b)), *canonical(&a));
}

#[test]
fn a_device_still_on_schema_1_merges_with_one_already_migrated() {
    // A migrated and edited; B's conflict copy is the old file, migrated on arrival
    let mut a = Device::new(1, &migrated(schema_1()));
    a.edit("b", |f| f.password = "pw4".into());
    let b = migrated(schema_1());

    let e = one(&both_ways(&a.data, &b), "b");
    assert_eq!(e.password, "pw4");
    let history: Vec<&str> = e.password_history.iter().map(|p| p.password.as_str()).collect();
    assert_eq!(history, ["pw1", "pw2", "pw3"]);
}

#[test]
fn a_schema_0_vault_migrates() {
    // The original format: no kind, email, folders or tombstones
    let v0 = json!({
        "entries": [{
            "id": "1", "name": "Old", "password": "pw", "url": null, "notes": null,
            "created_at": 5, "updated_at": 6
        }]
    });
    let data = migrated(v0);
    let e = one(&data, "1");
    assert_eq!((e.name.as_str(), e.password.as_str(), e.email.as_str()), ("Old", "pw", ""));
    assert_eq!(e.updated_at, 6);
}

// ── Newer fields ──────────────────────────────────────────────────────────────

#[test]
fn fields_from_a_newer_build_survive_a_save_from_this_one() {
    let mut d = Device::new(1, &VaultData::default());
    let now = d.now(1);
    d.data.add_folder("f".into(), "Work".into(), now).unwrap();
    d.add("1", "Site", "pw");

    // A newer build adds a field to the vault, an entry and a folder
    let mut file = serde_json::to_value(&d.data).unwrap();
    file["sync_settings"] = json!({ "interval": 30 });
    file["entries"]["records"]["1"]["passkey"] = json!([[1, 0, 9], "secret-key"]);
    file["folders"]["records"]["f"]["colour"] = json!("blue");
    let theirs: VaultData = serde_json::from_value(file).unwrap();

    // This build loads it, merges, edits and saves
    let mut ours = Device::new(2, &theirs);
    ours.data = merged(d.data.clone(), theirs);
    ours.edit("1", |f| f.notes = Some("edited by the older build".into()));
    let saved = serde_json::to_value(&ours.data).unwrap();

    assert_eq!(saved["sync_settings"], json!({ "interval": 30 }));
    assert_eq!(saved["entries"]["records"]["1"]["passkey"], json!([[1, 0, 9], "secret-key"]));
    assert_eq!(saved["folders"]["records"]["f"]["colour"], json!("blue"));
    assert_eq!(one(&ours.data, "1").notes.as_deref(), Some("edited by the older build"));
}

#[test]
fn merging_is_unaffected_by_which_copy_carries_a_newer_field() {
    let mut d = Device::new(1, &VaultData::default());
    d.add("1", "Site", "pw");
    let mut with_field = serde_json::to_value(&d.data).unwrap();
    with_field["entries"]["records"]["1"]["passkey"] = json!("k");
    let with_field: VaultData = serde_json::from_value(with_field).unwrap();

    let mut m = d.data.clone();
    m.merge(with_field.clone());
    assert_eq!(*canonical(&m), *canonical(&merged(with_field, d.data.clone())));
}
