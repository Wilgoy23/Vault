//! A sync folder is untrusted storage: whatever bytes a file holds, decoding
//! must return an error, never panic. This runs on stable in every `cargo
//! test`; `fuzz/` drives the same decoders with cargo-fuzz for longer runs.

use folder_crdt::codec::Envelope;
use folder_crdt::vault::file::FileCodec;
use folder_crdt::vault::{Fields, VaultData};
use folder_crdt::{Cipher, Codec, Hlc, Json, KeyMismatch, StoreError};
use proptest::prelude::*;
use zeroize::Zeroizing;

/// Identity "cipher" with a check byte, so mutated plaintext reaches the
/// record decoders instead of failing authentication.
struct Clear;

impl Cipher for Clear {
    fn seal(&self, plaintext: &[u8]) -> Result<Vec<u8>, StoreError> {
        Ok(std::iter::once(0).chain(plaintext.iter().copied()).collect())
    }
    fn open(&self, sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, KeyMismatch> {
        match sealed.split_first() {
            Some((0, rest)) => Ok(Zeroizing::new(rest.to_vec())),
            _ => Err(KeyMismatch),
        }
    }
}

fn sample() -> VaultData {
    let mut data = VaultData::default();
    let t = |ms| Hlc::new(ms, 0, 7);
    let folder = data.add_folder("f".into(), "Money".into(), t(1)).unwrap();
    let fields = |pw: &str| Fields {
        name: "Bank".into(),
        username: Some("me".into()),
        email: "me@example.com".into(),
        password: pw.into(),
        url: Some("bank.example".into()),
        notes: None,
        folder_id: Some(folder.id.clone()),
        totp_secret: None,
    };
    data.add_entry("e".into(), fields("pw1"), t(2)).unwrap();
    data.update_entry("e", fields("pw2"), t(3)).unwrap();
    data.mark_entry_used("e", t(4)).unwrap();
    data.add_entry("gone".into(), fields("x"), t(5)).unwrap();
    data.delete_entry("gone", t(6)).unwrap();
    data
}

const LEGACY: &str = r#"{"entries":[{"id":"1","name":"Old","email":"a","password":"pw","url":null,"notes":null,
"created_at":5,"updated_at":6,"password_changed_at":6,"password_history":[{"password":"p0","replaced_at":6}],
"use_count":2}],"folders":[{"id":"f","name":"F","updated_at":1}],"deleted":[{"id":"x","deleted_at":3}]}"#;

#[derive(Debug, Clone)]
enum Edit {
    Flip(usize, u8),
    Insert(usize, u8),
    Delete(usize),
}

fn edits() -> impl Strategy<Value = Vec<Edit>> {
    // JSON's structural bytes and digits are likelier to reach deep code
    let byte = prop_oneof![
        3 => prop::sample::select(b"{}[]\",:0123456789-.eEtfn ".to_vec()),
        1 => any::<u8>(),
    ];
    let edit = prop_oneof![
        (any::<usize>(), byte.clone()).prop_map(|(i, b)| Edit::Flip(i, b)),
        (any::<usize>(), byte).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
    ];
    prop::collection::vec(edit, 1..6)
}

fn apply(mut bytes: Vec<u8>, edits: &[Edit]) -> Vec<u8> {
    for e in edits {
        let n = bytes.len().max(1);
        match *e {
            Edit::Flip(i, b) if !bytes.is_empty() => bytes[i % n] = b,
            Edit::Insert(i, b) => bytes.insert(i % (bytes.len() + 1), b),
            Edit::Delete(i) if !bytes.is_empty() => {
                bytes.remove(i % n);
            }
            _ => {}
        }
    }
    bytes
}

/// A schema 2 file or a legacy one around `plaintext`, sealed with [`Clear`].
fn vault_file(plaintext: &[u8], legacy: bool) -> Vec<u8> {
    use base64::Engine;
    let sealed = Clear.seal(plaintext).unwrap();
    let ciphertext = base64::engine::general_purpose::STANDARD.encode(sealed);
    let file = if legacy {
        serde_json::json!({ "version": 1, "salt": "", "ciphertext": ciphertext })
    } else {
        serde_json::json!({
            "version": 2, "magic": "folder-crdt", "format": 1, "app": "vault", "schema": 2,
            "salt": "", "ciphertext": ciphertext
        })
    };
    file.to_string().into_bytes()
}

#[test]
fn the_unmutated_inputs_decode() {
    let current = serde_json::to_vec(&sample()).unwrap();
    FileCodec::new(Clear).decode(&vault_file(&current, false)).unwrap();
    FileCodec::new(Clear).decode(&vault_file(LEGACY.as_bytes(), true)).unwrap();
    let plain = Json::<VaultData>::new("vault", 2);
    plain.decode(&plain.encode(&sample()).unwrap()).unwrap();
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 3_000, ..ProptestConfig::default() })]

    #[test]
    fn mutated_records_never_panic(edits in edits()) {
        let current = apply(serde_json::to_vec(&sample()).unwrap(), &edits);
        let _ = FileCodec::new(Clear).decode(&vault_file(&current, false));
        let legacy = apply(LEGACY.as_bytes().to_vec(), &edits);
        let _ = FileCodec::new(Clear).decode(&vault_file(&legacy, true));
    }

    #[test]
    fn mutated_files_never_panic(edits in edits()) {
        let plain = Json::<VaultData>::new("vault", 2);
        let file = apply(plain.encode(&sample()).unwrap(), &edits);
        let _ = plain.decode(&file);
        let _ = Envelope::parse(&file);
        let sealed = apply(vault_file(&serde_json::to_vec(&sample()).unwrap(), false), &edits);
        let _ = FileCodec::new(Clear).decode(&sealed);
    }

    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..512)) {
        let _ = Json::<VaultData>::new("vault", 2).decode(&bytes);
        let _ = FileCodec::new(Clear).decode(&bytes);
    }
}
