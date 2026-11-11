//! Fields this build doesn't know, kept so that saving from an older build
//! doesn't erase what a newer one wrote.

use std::collections::{btree_map, BTreeMap};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use zeroize::{Zeroize, Zeroizing};

use crate::merge::Merge;

/// The fields of a record that this build has no name for. Put it in a
/// record as `#[serde(flatten)] extra: Unknown` and merge it with the rest.
///
/// This build can't know how a newer one merges these fields, so when two
/// copies hold different values for one, the one whose JSON text sorts last
/// wins: a join, so devices still agree. A newer build that adds a field
/// should make it a field type, whose clock comes first in its JSON.
///
/// Wiped from memory when dropped, since a newer field may hold a secret.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Unknown(BTreeMap<String, Value>);

// serde_json values never hold NaN, so equality is reflexive
impl Eq for Unknown {}

impl Unknown {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn get(&self, field: &str) -> Option<&Value> {
        self.0.get(field)
    }

    pub fn insert(&mut self, field: String, value: Value) {
        self.0.insert(field, value);
    }
}

impl Merge for Unknown {
    fn merge(&mut self, mut other: Self) {
        for (field, theirs) in std::mem::take(&mut other.0) {
            match self.0.entry(field) {
                btree_map::Entry::Vacant(e) => {
                    e.insert(theirs);
                }
                btree_map::Entry::Occupied(mut e) => {
                    if *text(&theirs) > *text(e.get()) {
                        wipe(e.insert(theirs));
                    } else {
                        wipe(theirs);
                    }
                }
            }
        }
    }
}

impl Zeroize for Unknown {
    fn zeroize(&mut self) {
        for (mut field, value) in std::mem::take(&mut self.0) {
            field.zeroize();
            wipe(value);
        }
    }
}

impl Drop for Unknown {
    fn drop(&mut self) {
        self.zeroize();
    }
}

/// `value` as JSON text, the order [`Unknown`] merges by.
fn text(value: &Value) -> Zeroizing<String> {
    Zeroizing::new(value.to_string())
}

/// Overwrites every string in `value` before it is freed.
fn wipe(mut value: Value) {
    match &mut value {
        Value::String(s) => s.zeroize(),
        Value::Array(items) => items.drain(..).for_each(wipe),
        Value::Object(map) => {
            let fields = std::mem::take(map);
            for (mut k, v) in fields {
                k.zeroize();
                wipe(v);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn of(v: Value) -> Unknown {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn merging_keeps_every_field_and_picks_the_same_value_either_way() {
        let a = of(json!({"only_a": 1, "both": [5, "a"]}));
        let b = of(json!({"only_b": true, "both": [7, "b"]}));
        let ab = crate::merged(a.clone(), b.clone());
        let ba = crate::merged(b, a);
        assert_eq!(ab, ba);
        assert_eq!(ab, of(json!({"only_a": 1, "only_b": true, "both": [7, "b"]})));
    }
}
