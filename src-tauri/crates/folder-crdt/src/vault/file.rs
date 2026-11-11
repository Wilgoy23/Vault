//! Vault's file, `vault.enc`: a [`Sealed`] file, read as well in the two
//! layouts Vault wrote before schema 2.
//!
//! Every layout is a JSON object with the Argon2id cost (`kdf`, absent in
//! schema 0) and salt (`salt`) in cleartext, and the AES-GCM sealed vault
//! base64-encoded under `ciphertext`. Schema 2 adds folder-crdt's header and
//! keeps `version`, which builds before it read to refuse newer files.

use std::path::PathBuf;

use crate::codec::{Cipher, Codec, Envelope, Sealed, StoreError};
use crate::format::{FormatError, FORMAT};
use crate::FolderStore;

use super::{legacy, VaultData, APP, SCHEMA};

/// Reads and writes `vault.enc` with Vault's [`Cipher`].
pub struct FileCodec<C> {
    sealed: Sealed<VaultData, C>,
}

impl<C: Cipher> FileCodec<C> {
    pub fn new(cipher: C) -> Self {
        FileCodec { sealed: Sealed::new(APP, SCHEMA, cipher).header_field("version", SCHEMA) }
    }

    pub fn cipher(&self) -> &C {
        self.sealed.cipher()
    }
}

/// The schema a file says it was written at, by the field every build of
/// Vault has read.
fn version(envelope: &Envelope) -> u64 {
    envelope.params.get("version").and_then(|v| v.as_u64()).unwrap_or(0)
}

/// Checks a file is one this build can open, without opening it.
pub fn check(bytes: &[u8]) -> Result<(), StoreError> {
    check_envelope(&Envelope::parse(bytes)?)
}

fn check_envelope(envelope: &Envelope) -> Result<(), StoreError> {
    let version = version(envelope);
    if version > u64::from(SCHEMA) {
        let format = envelope.header.as_ref().map_or(FORMAT, |h| h.format);
        let schema = u32::try_from(version).unwrap_or(u32::MAX);
        return Err(FormatError::Newer { format, schema }.into());
    }
    match &envelope.header {
        Some(_) => envelope.check(APP, SCHEMA).map(|_| ()),
        None if version < u64::from(SCHEMA) => Ok(()),
        None => Err(StoreError::Corrupt("a schema 2 file without its header".into())),
    }
}

impl<C: Cipher> Codec for FileCodec<C> {
    type Data = VaultData;

    fn encode(&self, data: &VaultData) -> Result<Vec<u8>, StoreError> {
        self.sealed.encode(data)
    }

    /// Schema 0 and 1 files are converted to schema 2 as they are read.
    fn decode(&self, bytes: &[u8]) -> Result<VaultData, StoreError> {
        let envelope = Envelope::parse(bytes)?;
        check_envelope(&envelope)?;
        if envelope.header.is_some() {
            return self.sealed.open(&envelope);
        }
        let plaintext = self.cipher().open(&envelope.payload)?;
        let old: legacy::VaultData =
            serde_json::from_slice(&plaintext).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        Ok(legacy::migrate(&old))
    }
}

/// The store for the vault kept in `dir`: `vault.enc`, its conflict copies
/// beside it, and exports named `vault-backup…` left alone.
pub fn store<C: Cipher>(dir: impl Into<PathBuf>, cipher: C) -> FolderStore<FileCodec<C>> {
    FolderStore::new(dir, "vault", "enc", FileCodec::new(cipher))
}
