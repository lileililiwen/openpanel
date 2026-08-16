//! AES-256-GCM encryption at rest for container runtime secrets
//! (registry passwords). The master key is supplied by config;
//! each record has its own random 12-byte nonce. Stored
//! representation is `hex(nonce) || ":" || hex(ciphertext)` so
//! the column is pure ASCII (matches `Agents.md` §6 and the
//! `databases::crypto` layout).
//!
//! This is kept as a private per-context module by design —
//! the bounded-context boundary forbids reaching into
//! `openpanel_app::databases::crypto` from another bounded
//! context.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use openpanel_domain::ContainerRuntimeError;
use rand::RngCore;

/// AES-GCM nonce length in bytes (96-bit, the standard for AES-256-GCM).
pub const NONCE_LEN: usize = 12;
/// AES-256 master key length in bytes (256-bit).
pub const KEY_LEN: usize = 32;

/// Decode a base64 master key into raw bytes.
///
/// Returns `ContainerRuntimeError::InvalidCipher` when the input
/// is empty, not valid base64, or not exactly `KEY_LEN` bytes.
pub fn decode_master_key(s: &str) -> Result<[u8; KEY_LEN], ContainerRuntimeError> {
    if s.is_empty() {
        return Err(ContainerRuntimeError::InvalidCipher(
            "master key missing".into(),
        ));
    }
    let raw = B64
        .decode(s)
        .map_err(|e| ContainerRuntimeError::InvalidCipher(format!("base64 decode: {e}")))?;
    if raw.len() != KEY_LEN {
        return Err(ContainerRuntimeError::InvalidCipher(format!(
            "master key length {} != {KEY_LEN}",
            raw.len()
        )));
    }
    let mut out = [0u8; KEY_LEN];
    out.copy_from_slice(&raw);
    Ok(out)
}

fn cipher(key_bytes: &[u8; KEY_LEN]) -> Aes256Gcm {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    Aes256Gcm::new(key)
}

/// Encrypt `plaintext` with the given master key. Returns the
/// `nonce_hex:ciphertext_hex` form suitable for SQLite TEXT
/// columns and matching the layout used by the other bounded
/// contexts.
pub fn encrypt_secret(
    master_key: &[u8; KEY_LEN],
    plaintext: &str,
) -> Result<String, ContainerRuntimeError> {
    let cipher = cipher(master_key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| ContainerRuntimeError::InvalidCipher(format!("encrypt: {e}")))?;
    Ok(format!("{}:{}", hex::encode(nonce_bytes), hex::encode(ct)))
}

/// Decrypt a `nonce_hex:ciphertext_hex` string back to plaintext.
pub fn decrypt_secret(
    master_key: &[u8; KEY_LEN],
    stored: &str,
) -> Result<String, ContainerRuntimeError> {
    let (nonce_hex, ct_hex) = stored
        .split_once(':')
        .ok_or_else(|| ContainerRuntimeError::InvalidCipher("missing `:` separator".into()))?;
    let nonce_bytes = hex::decode(nonce_hex).map_err(|_| {
        ContainerRuntimeError::InvalidCipher(format!("bad nonce hex `{nonce_hex}`"))
    })?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(ContainerRuntimeError::InvalidCipher(format!(
            "nonce length {} != {NONCE_LEN}",
            nonce_bytes.len()
        )));
    }
    let ct = hex::decode(ct_hex)
        .map_err(|_| ContainerRuntimeError::InvalidCipher("bad ciphertext hex".into()))?;
    let cipher = cipher(master_key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|e| ContainerRuntimeError::InvalidCipher(format!("decrypt: {e}")))?;
    String::from_utf8(pt).map_err(|_| ContainerRuntimeError::CredentialDecode)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = [0x42u8; KEY_LEN];
        let stored = encrypt_secret(&key, "registry-pat-secret").unwrap();
        let pt = decrypt_secret(&key, &stored).unwrap();
        assert_eq!(pt, "registry-pat-secret");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [0x42u8; KEY_LEN];
        let mut stored = encrypt_secret(&key, "secret").unwrap();
        let last = stored.len() - 1;
        let mut bytes = stored.into_bytes();
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        stored = String::from_utf8(bytes).unwrap();
        assert!(decrypt_secret(&key, &stored).is_err());
    }

    #[test]
    fn missing_master_key() {
        assert!(matches!(
            decode_master_key(""),
            Err(ContainerRuntimeError::InvalidCipher(_))
        ));
    }

    #[test]
    fn wrong_key_length() {
        let short = B64.encode([1u8; 16]);
        assert!(decode_master_key(&short).is_err());
    }
}
