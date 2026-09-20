//! Windows Hello quick unlock.
//!
//! Hello cannot derive a key from a fingerprint or a face — it gates access to
//! a key pair held by the TPM. So the vault key is never handed to Hello and
//! never written to disk. Instead:
//!
//!   1. On unlock with the master password, a random challenge is generated
//!      and signed by a Hello-protected credential. The signature is hashed
//!      into a wrapping key, which encrypts the vault key in memory. Both the
//!      signature and the wrapping key are then dropped.
//!   2. While the vault is locked, only the wrapped key and the challenge
//!      remain in the process. Unwrapping them needs the same signature, and
//!      the TPM will not produce it without a biometric or PIN gesture.
//!
//! A memory dump of a locked Vault therefore yields nothing usable, and
//! nothing survives the process exiting. Every failure path falls back to the
//! master password, so a machine with no Hello enrolled simply never offers
//! quick unlock.

use windows::core::HSTRING;
use windows::Security::Credentials::{
    KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
};
use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
use zeroize::Zeroizing;

/// Identifies Vault's credential in the user's Hello store. Changing this
/// string orphans the old credential and forces a new enrolment.
const CREDENTIAL_NAME: &str = "com.willg.vault.quick-unlock";

/// True if this machine has Hello set up and usable right now.
pub fn is_available() -> bool {
    KeyCredentialManager::IsSupportedAsync()
        .and_then(|op| op.get())
        .unwrap_or(false)
}

fn to_buffer(bytes: &[u8]) -> Result<IBuffer, String> {
    let writer = DataWriter::new().map_err(|e| e.to_string())?;
    writer.WriteBytes(bytes).map_err(|e| e.to_string())?;
    writer.DetachBuffer().map_err(|e| e.to_string())
}

fn from_buffer(buffer: &IBuffer) -> Result<Zeroizing<Vec<u8>>, String> {
    let len = buffer.Length().map_err(|e| e.to_string())? as usize;
    let mut out = Zeroizing::new(vec![0u8; len]);
    let reader = DataReader::FromBuffer(buffer).map_err(|e| e.to_string())?;
    reader.ReadBytes(&mut out).map_err(|e| e.to_string())?;
    Ok(out)
}

/// Asks Hello to sign `challenge`, prompting the user for their face,
/// fingerprint or PIN, and hashes the signature into a 256-bit wrapping key.
///
/// Hello credentials sign with RSASSA-PKCS1-v1_5, which is deterministic, so
/// the same challenge always yields the same key. That is what lets the vault
/// key be unwrapped later without storing anything derived from the user.
pub fn derive_wrapping_key(challenge: &[u8]) -> Result<Zeroizing<[u8; 32]>, String> {
    let name = HSTRING::from(CREDENTIAL_NAME);

    // Reuse the existing credential; create one on first use.
    let opened = KeyCredentialManager::OpenAsync(&name)
        .and_then(|op| op.get())
        .map_err(|e| e.to_string())?;

    let credential = match opened.Status().map_err(|e| e.to_string())? {
        KeyCredentialStatus::Success => opened.Credential().map_err(|e| e.to_string())?,
        _ => {
            let created = KeyCredentialManager::RequestCreateAsync(
                &name,
                KeyCredentialCreationOption::ReplaceExisting,
            )
            .and_then(|op| op.get())
            .map_err(|e| e.to_string())?;
            match created.Status().map_err(|e| e.to_string())? {
                KeyCredentialStatus::Success => created.Credential().map_err(|e| e.to_string())?,
                status => return Err(format!("Windows Hello is not available ({status:?})")),
            }
        }
    };

    // This is the call that prompts for the gesture.
    let signed = credential
        .RequestSignAsync(&to_buffer(challenge)?)
        .map_err(|e| e.to_string())?
        .get()
        .map_err(|e| e.to_string())?;

    match signed.Status().map_err(|e| e.to_string())? {
        KeyCredentialStatus::Success => {}
        KeyCredentialStatus::UserCanceled => return Err("Cancelled".into()),
        status => return Err(format!("Windows Hello did not sign in ({status:?})")),
    }

    let signature = from_buffer(&signed.Result().map_err(|e| e.to_string())?)?;
    if signature.is_empty() {
        return Err("Windows Hello returned an empty signature".into());
    }

    use sha2::{Digest, Sha256};
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(&Sha256::digest(&signature[..]));
    Ok(key)
}

/// Removes Vault's Hello credential, so the next arm enrols afresh.
pub fn forget() {
    let _ = KeyCredentialManager::DeleteAsync(&HSTRING::from(CREDENTIAL_NAME))
        .and_then(|op| op.get());
}
