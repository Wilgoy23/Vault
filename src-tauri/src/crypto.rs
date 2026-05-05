use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::{Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use zeroize::Zeroize;

const SALT_LEN: usize = 32;
const KEY_LEN: usize = 32;

/// Derives a 256-bit key from a master password + salt using Argon2id.
/// The key is returned as a fixed array — caller is responsible for zeroizing it.
pub fn derive_key(password: &str, salt: &[u8]) -> Result<[u8; KEY_LEN], String> {
    let params = Params::new(65536, 3, 1, Some(KEY_LEN))
        .map_err(|e| format!("Argon2 params error: {e}"))?;

    let argon2 = Argon2::new(argon2::Algorithm::Argon2id, Version::V0x13, params);

    let mut key = [0u8; KEY_LEN];
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Key derivation failed: {e}"))?;

    Ok(key)
}

/// Generates a fresh random salt. Call once when creating a new vault.
pub fn generate_salt() -> [u8; SALT_LEN] {
    let mut salt = [0u8; SALT_LEN];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut salt);
    salt
}

/// Encrypts plaintext with AES-256-GCM.
/// Returns base64(nonce || ciphertext).
pub fn encrypt(plaintext: &[u8], key: &[u8; KEY_LEN]) -> Result<String, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {e}"))?;

    // Prepend nonce to ciphertext, then base64-encode the whole thing
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(B64.encode(&combined))
}

/// Decrypts a base64(nonce || ciphertext) produced by `encrypt`.
pub fn decrypt(encoded: &str, key: &[u8; KEY_LEN]) -> Result<Vec<u8>, String> {
    let combined = B64
        .decode(encoded)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;

    if combined.len() < 12 {
        return Err("Ciphertext too short".into());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "Decryption failed — wrong password or corrupted vault".into())
}

/// Zeroizes a key buffer. Call this whenever you're done with a key.
pub fn wipe_key(key: &mut [u8; KEY_LEN]) {
    key.zeroize();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_derivation_is_deterministic() {
        let password = "my-master-password";
        let salt = generate_salt();
        let key1 = derive_key(password, &salt).unwrap();
        let key2 = derive_key(password, &salt).unwrap();
        assert_eq!(key1, key2);
    }

    #[test]
    fn different_passwords_produce_different_keys() {
        let salt = generate_salt();
        let key1 = derive_key("password-one", &salt).unwrap();
        let key2 = derive_key("password-two", &salt).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn different_salts_produce_different_keys() {
        let password = "same-password";
        let salt1 = generate_salt();
        let salt2 = generate_salt();
        let key1 = derive_key(password, &salt1).unwrap();
        let key2 = derive_key(password, &salt2).unwrap();
        assert_ne!(key1, key2);
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let key = derive_key("test-password", &generate_salt()).unwrap();
        let plaintext = b"super secret data";
        let encrypted = encrypt(plaintext, &key).unwrap();
        let decrypted = decrypt(&encrypted, &key).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn encrypt_produces_different_ciphertext_each_time() {
        // AES-GCM uses a random nonce, so two encryptions of the same
        // plaintext should never produce the same output
        let key = derive_key("test-password", &generate_salt()).unwrap();
        let plaintext = b"same plaintext";
        let c1 = encrypt(plaintext, &key).unwrap();
        let c2 = encrypt(plaintext, &key).unwrap();
        assert_ne!(c1, c2);
    }

    #[test]
    fn decrypt_fails_with_wrong_key() {
        let key1 = derive_key("correct-password", &generate_salt()).unwrap();
        let key2 = derive_key("wrong-password", &generate_salt()).unwrap();
        let encrypted = encrypt(b"secret", &key1).unwrap();
        let result = decrypt(&encrypted, &key2);
        assert!(result.is_err());
    }

    #[test]
    fn decrypt_fails_on_tampered_ciphertext() {
        let key = derive_key("test-password", &generate_salt()).unwrap();
        let mut encrypted = encrypt(b"secret data", &key).unwrap();
        // Flip a character near the end to simulate tampering
        let len = encrypted.len();
        encrypted.replace_range(len - 2..len, "XX");
        let result = decrypt(&encrypted, &key);
        assert!(result.is_err());
    }

    #[test]
    fn wipe_key_zeroes_the_buffer() {
        let mut key = derive_key("test-password", &generate_salt()).unwrap();
        wipe_key(&mut key);
        assert_eq!(key, [0u8; 32]);
    }
}