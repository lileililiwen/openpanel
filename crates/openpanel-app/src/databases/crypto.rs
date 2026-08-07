//! AES-256-GCM password encryption at rest. The master key is supplied
//! by config; each record has its own random 12-byte nonce. Stored
//! representation is `hex(nonce) || hex(ciphertext)` so the column is
//! pure ASCII.

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use rand::RngCore;

use openpanel_domain::databases::error::DatabaseError;

pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;

/// Decode a base64 master key into raw bytes. Returns
/// `DatabaseError::MasterKeyMissing` if the input is empty.
pub fn decode_master_key(s: &str) -> Result<[u8; KEY_LEN], DatabaseError> {
    if s.is_empty() {
        return Err(DatabaseError::MasterKeyMissing);
    }
    let raw = B64
        .decode(s)
        .map_err(|e| DatabaseError::Encryption(format!("base64 decode: {e}")))?;
    if raw.len() != KEY_LEN {
        return Err(DatabaseError::Encryption(format!(
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

/// Encrypt `plaintext` with the given master key. Returns
/// `nonce || ciphertext` as a hex string suitable for SQLite storage.
pub fn encrypt_to_storage(
    master_key: &[u8; KEY_LEN],
    plaintext: &str,
) -> Result<String, DatabaseError> {
    let cipher = cipher(master_key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| DatabaseError::Encryption(e.to_string()))?;
    Ok(format!("{}:{}", hex_encode(&nonce_bytes), hex_encode(&ct)))
}

/// Decrypt a `nonce:ciphertext` hex string back to plaintext.
pub fn decrypt_from_storage(
    master_key: &[u8; KEY_LEN],
    stored: &str,
) -> Result<String, DatabaseError> {
    let (nonce_hex, ct_hex) = stored
        .split_once(':')
        .ok_or_else(|| DatabaseError::Decryption("missing `:` separator".into()))?;
    let nonce_bytes = hex_decode(nonce_hex)
        .ok_or_else(|| DatabaseError::Decryption(format!("bad nonce hex `{nonce_hex}`")))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(DatabaseError::Decryption(format!(
            "nonce length {} != {NONCE_LEN}",
            nonce_bytes.len()
        )));
    }
    let ct = hex_decode(ct_hex)
        .ok_or_else(|| DatabaseError::Decryption("bad ciphertext hex".to_string()))?;
    let cipher = cipher(master_key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|e| DatabaseError::Decryption(e.to_string()))?;
    String::from_utf8(pt).map_err(|e| DatabaseError::Decryption(e.to_string()))
}

fn hex_encode(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

fn hex_decode(s: &str) -> Option<Vec<u8>> {
    hex::decode(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let key = [0x42u8; KEY_LEN];
        let stored = encrypt_to_storage(&key, "hunter2-correct-horse").unwrap();
        let pt = decrypt_from_storage(&key, &stored).unwrap();
        assert_eq!(pt, "hunter2-correct-horse");
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let key = [0x42u8; KEY_LEN];
        let mut stored = encrypt_to_storage(&key, "secret").unwrap();
        // Flip one char in the ciphertext portion.
        let colon = stored.rfind(':').unwrap();
        let mut bytes = stored.into_bytes();
        let last = bytes.len() - 1;
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        stored = String::from_utf8(bytes).unwrap();
        let _ = colon;
        let r = decrypt_from_storage(&key, &stored);
        assert!(r.is_err());
    }

    #[test]
    fn missing_master_key() {
        assert!(matches!(
            decode_master_key(""),
            Err(DatabaseError::MasterKeyMissing)
        ));
    }

    #[test]
    fn wrong_key_length() {
        let short = B64.encode([1u8; 16]);
        assert!(decode_master_key(&short).is_err());
    }
}
