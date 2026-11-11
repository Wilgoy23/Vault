//! Feeds arbitrary bytes to every decoder a synced file reaches. Run with
//! `cargo +nightly fuzz run decode` from crates/folder-crdt.
#![no_main]

use folder_crdt::codec::Envelope;
use folder_crdt::vault::file::FileCodec;
use folder_crdt::vault::VaultData;
use folder_crdt::{Cipher, Codec, Json, KeyMismatch, StoreError};
use libfuzzer_sys::fuzz_target;
use zeroize::Zeroizing;

/// Opens anything starting with a 0 byte, so input reaches the record
/// decoders instead of stopping at authentication.
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

fuzz_target!(|bytes: &[u8]| {
    let _ = Envelope::parse(bytes);
    let _ = Json::<VaultData>::new("vault", 2).decode(bytes);
    let _ = FileCodec::new(Clear).decode(bytes);
    // The records themselves, as they'd arrive after decryption
    let _ = serde_json::from_slice::<VaultData>(bytes);
    let _ = serde_json::from_slice::<folder_crdt::vault::legacy::VaultData>(bytes);
});
