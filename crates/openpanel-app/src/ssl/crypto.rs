//! AES-256-GCM encryption of TLS private keys at rest.
//!
//! The cipher and storage format are identical to the databases
//! module's password envelope: `hex(nonce) || ":" || hex(ciphertext)`
//! in a single string. Reusing the format keeps a single key
//! management story (one master key, one audit trail, one rotation
//! procedure) and lets the key column be stored alongside the cert
//! in the same row.

// `encrypt_to_storage` / `decrypt_from_storage` use these internally.
use openpanel_domain::ssl::error::SslError;

use super::super::databases::crypto::{KEY_LEN, decode_master_key};

/// Encrypt a PEM-encoded private key with the panel master key.
/// Returns the ciphertext as raw bytes (UTF-8 of the
/// `nonce:ciphertext` hex string) suitable for SQLite BLOB storage.
pub fn encrypt_key_pem(master_key: &[u8; KEY_LEN], key_pem: &str) -> Result<Vec<u8>, SslError> {
    let stored = super::super::databases::crypto::encrypt_to_storage(master_key, key_pem)
        .map_err(|e| SslError::Encryption(e.to_string()))?;
    Ok(stored.into_bytes())
}

/// Decrypt a previously-encrypted private key back to PEM. Accepts
/// either raw bytes or a `&str` ciphertext.
pub fn decrypt_key_pem(master_key: &[u8; KEY_LEN], stored: &[u8]) -> Result<String, SslError> {
    let stored_str = std::str::from_utf8(stored)
        .map_err(|_| SslError::Decryption("ciphertext is not valid utf-8".into()))?;
    super::super::databases::crypto::decrypt_from_storage(master_key, stored_str)
        .map_err(|e| SslError::Decryption(e.to_string()))
}

/// Convenience: decode a base64 master key into the raw 32 bytes the
/// other helpers need.
pub fn master_key_from_base64(s: &str) -> Result<[u8; KEY_LEN], SslError> {
    decode_master_key(s).map_err(|e| SslError::Encryption(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_pem_through_storage() {
        let key = [0x42u8; KEY_LEN];
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIBV...\n-----END PRIVATE KEY-----\n";
        let stored = encrypt_key_pem(&key, pem).unwrap();
        // Stored form is NOT valid PEM bytes.
        assert!(
            !std::str::from_utf8(&stored)
                .unwrap()
                .starts_with("-----BEGIN")
        );
        assert!(std::str::from_utf8(&stored).unwrap().contains(':'));
        let back = decrypt_key_pem(&key, &stored).unwrap();
        assert_eq!(back, pem);
    }

    #[test]
    fn different_nonces_per_encryption() {
        let key = [0x42u8; KEY_LEN];
        let pem = "-----BEGIN PRIVATE KEY-----\nX\n-----END PRIVATE KEY-----\n";
        let a = encrypt_key_pem(&key, pem).unwrap();
        let b = encrypt_key_pem(&key, pem).unwrap();
        assert_ne!(a, b, "nonce reuse would be catastrophic for GCM");
    }
}
