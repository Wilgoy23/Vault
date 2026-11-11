//! The PRD's core scenarios, on a login record built from the generic
//! types: what Vault's entries become in phase 3.
//!
//! Scenario 5 (conflict copies merged and removed) needs the folder store
//! from phase 4; until then it is covered by `fs::conflict_copies` tests and
//! Vault's own `sync` test.

use folder_crdt::*;

type Clock = u64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Merge, Record)]
#[record(clock = Clock)]
struct Login {
    name: Lww<String, Clock>,
    url: Lww<Option<String>, Clock>,
    /// The current password and the 10 before it
    password: LwwHistory<String, Clock, 11>,
    uses: Counter,
    last_used: Max<Clock>,
}

fn login(name: &str, password: &str, at: Clock) -> Login {
    Login {
        name: Lww::new(name.into(), at),
        url: Lww::new(None, at),
        password: LwwHistory::new(password.into(), at),
        ..Login::default()
    }
}

type Logins = RecordSet<&'static str, Login>;

/// Both merge orders, checked to agree, so each scenario also shows the
/// result doesn't depend on which device syncs first.
fn both_ways<T: Merge + Clone + PartialEq + std::fmt::Debug>(a: &T, b: &T) -> T {
    let ab = merged(a.clone(), b.clone());
    let ba = merged(b.clone(), a.clone());
    assert_eq!(ab, ba, "merge order changed the result");
    ab
}

#[test]
fn edits_to_different_records_both_survive() {
    let mut base = Logins::new();
    base.upsert("github", login("GitHub", "pw", 1));

    let mut laptop = base.clone();
    laptop.upsert("bank", login("Bank", "pw", 5));
    let mut phone = base.clone();
    phone.upsert("mail", login("Mail", "pw", 6));

    let merged = both_ways(&laptop, &phone);
    let ids: Vec<_> = merged.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, ["bank", "github", "mail"]);
}

#[test]
fn edits_to_different_fields_of_one_record_both_survive() {
    let mut base = Logins::new();
    base.upsert("github", login("GitHub", "pw", 1));

    let mut laptop = base.clone();
    laptop.get_mut(&"github").unwrap().name.set("GitHub (work)".into(), 5);
    let mut phone = base.clone();
    phone.get_mut(&"github").unwrap().url.set(Some("github.com".into()), 6);

    let merged = both_ways(&laptop, &phone);
    let g = merged.get(&"github").unwrap();
    assert_eq!(g.name.get().unwrap(), "GitHub (work)");
    assert_eq!(g.url.get().unwrap().as_deref(), Some("github.com"));
}

#[test]
fn the_same_field_picks_one_winner_and_keeps_the_loser_in_history() {
    let mut base = Logins::new();
    base.upsert("bank", login("Bank", "original", 1));

    let mut laptop = base.clone();
    laptop.get_mut(&"bank").unwrap().password.set("from-laptop".into(), 5);
    let mut phone = base.clone();
    phone.get_mut(&"bank").unwrap().password.set("from-phone".into(), 7);

    let merged = both_ways(&laptop, &phone);
    let pw = &merged.get(&"bank").unwrap().password;
    assert_eq!(pw.get().unwrap(), "from-phone");
    let history: Vec<&str> = pw.history().map(|(_, p)| p.as_str()).collect();
    assert_eq!(history, ["from-laptop", "original"]);
}

#[test]
fn a_delete_beats_earlier_edits_but_not_later_ones() {
    let mut base = Logins::new();
    base.upsert("old", login("Old", "pw", 1));
    base.upsert("kept", login("Kept", "pw", 1));

    let mut laptop = base.clone();
    laptop.delete("old", 10);
    laptop.delete("kept", 10);

    let mut phone = base.clone();
    phone.get_mut(&"old").unwrap().name.set("Edited before".into(), 8);
    phone.get_mut(&"kept").unwrap().name.set("Edited after".into(), 12);

    let merged = both_ways(&laptop, &phone);
    assert!(merged.get(&"old").is_none());
    assert_eq!(merged.get(&"kept").unwrap().name.get().unwrap(), "Edited after");
}

#[test]
fn under_delete_wins_a_later_edit_does_not_bring_a_record_back() {
    let mut base: RecordSet<&str, Login, DeleteWins> = RecordSet::new();
    base.upsert("gone", login("Gone", "pw", 1));

    let mut laptop = base.clone();
    laptop.delete("gone", 10);
    let mut phone = base.clone();
    phone.get_mut(&"gone").unwrap().name.set("Edited after".into(), 12);

    let merged = both_ways(&laptop, &phone);
    assert!(merged.get(&"gone").is_none());
    assert!(merged.is_empty());
}

#[test]
fn a_device_offline_for_months_does_not_bring_back_a_delete() {
    let mut base = Logins::new();
    base.upsert("old", login("Old", "pw", 1));

    // The tablet goes offline with its copy; everyone else deletes the
    // record and carries on editing other things for a long time
    let tablet = base.clone();
    let mut everyone = base.clone();
    everyone.delete("old", 100);
    for day in 0..180 {
        everyone.upsert("busy", login("Busy", &format!("pw{day}"), 200 + day));
    }

    let merged = both_ways(&everyone, &tablet);
    assert!(merged.get(&"old").is_none());
    assert!(merged.is_deleted(&"old"));
}

#[test]
fn uses_on_two_devices_at_once_are_both_counted() {
    let mut base = Logins::new();
    base.upsert("github", login("GitHub", "pw", 1));

    let mut laptop = base.clone();
    let g = laptop.get_mut(&"github").unwrap();
    g.uses.increment(1, 2);
    g.last_used.set(50);
    let mut phone = base.clone();
    let g = phone.get_mut(&"github").unwrap();
    g.uses.increment(2, 3);
    g.last_used.set(40);

    let merged = both_ways(&laptop, &phone);
    let g = merged.get(&"github").unwrap();
    // Vault's current merge takes the max and would report 3
    assert_eq!(g.uses.value(), 5);
    assert_eq!(g.last_used.get(), Some(&50));
}
