//! KEK (key-encryption-key) management for offsite backups.
//!
//! A per-install KEK is derived once from the operator passphrase
//! with Argon2id and salted by the panel's master-key fingerprint.
//! Backup payloads are encrypted with AES-256-GCM under that KEK,
//! so the off-site copies remain decryptable from a cold backup
//! with just the passphrase. The KEK is additionally wrapped with
//! the master key and stored in `backup_kek_wrappers`, letting the
//! panel decrypt without the passphrase during normal operation.

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Argon2, password_hash::SaltString};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use openpanel_domain::OffsiteBackupError;
use rand::RngCore;
use sha2::{Digest, Sha256};

/// AES-256-GCM key length (256-bit).
pub const KEK_LEN: usize = 32;
/// AES-GCM nonce length (96-bit).
pub const NONCE_LEN: usize = 12;
/// Argon2id parameters for KEK derivation. Deliberately
/// interactive-class: the KEK is derived once per panel install.
const ARGON2_M_COST: u32 = 65536;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 1;

/// Derives the 32-byte backup KEK from the operator passphrase and
/// the panel master-key fingerprint (hex). The fingerprint acts as
/// a deterministic salt so a cold restore can re-derive the same
/// KEK.
pub fn derive_kek(
    passphrase: &[u8],
    master_key_fingerprint: &str,
) -> Result<[u8; KEK_LEN], OffsiteBackupError> {
    derive_kek_with_params(
        passphrase,
        master_key_fingerprint,
        ARGON2_M_COST,
        ARGON2_T_COST,
        ARGON2_P_COST,
    )
}

/// Derive the KEK with explicit Argon2id parameters. Test suites
/// use a reduced cost; production uses `derive_kek`.
pub(crate) fn derive_kek_with_params(
    passphrase: &[u8],
    master_key_fingerprint: &str,
    m_cost: u32,
    t_cost: u32,
    p_cost: u32,
) -> Result<[u8; KEK_LEN], OffsiteBackupError> {
    if passphrase.is_empty() {
        return Err(OffsiteBackupError::InvalidSecret(
            "passphrase must not be empty".to_string(),
        ));
    }
    if master_key_fingerprint.len() < 16 {
        return Err(OffsiteBackupError::InvalidSecret(
            "master key fingerprint is too short to use as a salt".to_string(),
        ));
    }
    let raw_salt = hex::decode(master_key_fingerprint)
        .map_err(|_| OffsiteBackupError::InvalidSecret("fingerprint is not hex".to_string()))?;
    if raw_salt.len() < 8 || raw_salt.len() > 32 {
        return Err(OffsiteBackupError::InvalidSecret(format!(
            "fingerprint decodes to {} bytes; need 8..=32 for the salt",
            raw_salt.len()
        )));
    }
    let salt = SaltString::encode_b64(&raw_salt)
        .map_err(|e| OffsiteBackupError::InvalidSecret(format!("bad fingerprint salt: {e}")))?;
    let argon = Argon2::new(
        argon2::Algorithm::Argon2id,
        argon2::Version::V0x13,
        argon2::Params::new(m_cost, t_cost, p_cost, None)
            .map_err(|e| OffsiteBackupError::InvalidSecret(format!("argon2 params: {e}")))?,
    );
    let mut kek = [0u8; KEK_LEN];
    argon
        .hash_password_into(passphrase, salt.as_str().as_bytes(), &mut kek)
        .map_err(|e| OffsiteBackupError::InvalidSecret(format!("argon2 derive: {e}")))?;
    Ok(kek)
}

fn cipher(kek: &[u8; KEK_LEN]) -> Aes256Gcm {
    let key = Key::<Aes256Gcm>::from_slice(kek);
    Aes256Gcm::new(key)
}

/// Encrypt `plaintext` under the KEK. Returns the
/// `nonce_hex:ciphertext_hex` ASCII form used by the encrypted
/// persistence columns.
pub fn encrypt_payload(kek: &[u8; KEK_LEN], plaintext: &str) -> Result<String, OffsiteBackupError> {
    let cipher = cipher(kek);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| OffsiteBackupError::InvalidSecret(format!("encrypt: {e}")))?;
    Ok(format!("{}:{}", hex::encode(nonce_bytes), hex::encode(ct)))
}

/// Decrypt a `nonce_hex:ciphertext_hex` payload under the KEK.
/// Returns `OffsiteBackupError::CredentialDecrypt` on any tamper
/// detected by the AES-GCM authentication tag.
pub fn decrypt_payload(kek: &[u8; KEK_LEN], stored: &str) -> Result<String, OffsiteBackupError> {
    let (nonce_hex, ct_hex) = stored
        .split_once(':')
        .ok_or(OffsiteBackupError::CredentialDecrypt)?;
    let nonce_bytes = hex::decode(nonce_hex).map_err(|_| OffsiteBackupError::CredentialDecrypt)?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(OffsiteBackupError::CredentialDecrypt);
    }
    let ct = hex::decode(ct_hex).map_err(|_| OffsiteBackupError::CredentialDecrypt)?;
    let cipher = cipher(kek);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|_| OffsiteBackupError::CredentialDecrypt)?;
    String::from_utf8(pt).map_err(|_| OffsiteBackupError::CredentialDecrypt)
}

/// Wrap the KEK with the panel master key. Returns the wrapped
/// KEK bytes (hex) for `backup_kek_wrappers.wrapped_kek_hex`.
pub fn wrap_kek(
    master_key: &[u8; KEK_LEN],
    kek: &[u8; KEK_LEN],
) -> Result<String, OffsiteBackupError> {
    let cipher = cipher(master_key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, kek.as_slice())
        .map_err(|e| OffsiteBackupError::InvalidKek(format!("wrap: {e}")))?;
    Ok(format!("{}:{}", hex::encode(nonce_bytes), hex::encode(ct)))
}

/// Unwrap the KEK with the panel master key.
pub fn unwrap_kek(
    master_key: &[u8; KEK_LEN],
    wrapped_hex: &str,
) -> Result<[u8; KEK_LEN], OffsiteBackupError> {
    let (nonce_hex, ct_hex) = wrapped_hex
        .split_once(':')
        .ok_or(OffsiteBackupError::InvalidKek("missing separator".into()))?;
    let nonce_bytes = hex::decode(nonce_hex)
        .map_err(|_| OffsiteBackupError::InvalidKek("bad nonce hex".into()))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(OffsiteBackupError::InvalidKek("nonce length".into()));
    }
    let ct = hex::decode(ct_hex)
        .map_err(|_| OffsiteBackupError::InvalidKek("bad ciphertext hex".into()))?;
    let cipher = cipher(master_key);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let pt = cipher
        .decrypt(nonce, ct.as_slice())
        .map_err(|_| OffsiteBackupError::InvalidKek("tag mismatch".into()))?;
    if pt.len() != KEK_LEN {
        return Err(OffsiteBackupError::InvalidKek("unwrapped length".into()));
    }
    let mut out = [0u8; KEK_LEN];
    out.copy_from_slice(&pt);
    Ok(out)
}

/// Hex SHA-256 fingerprint of the master key, used as the Argon2id
/// salt for KEK derivation.
pub fn master_key_fingerprint(master_key: &[u8; KEK_LEN]) -> String {
    hex::encode(Sha256::digest(master_key))
}

/// Decode a base64 master key from config into raw bytes.
#[allow(dead_code)] // config bootstrap path; exercised by the tests below
pub fn decode_master_key(s: &str) -> Result<[u8; KEK_LEN], OffsiteBackupError> {
    if s.is_empty() {
        return Err(OffsiteBackupError::InvalidKek("master key missing".into()));
    }
    let raw = B64
        .decode(s)
        .map_err(|e| OffsiteBackupError::InvalidKek(format!("base64 decode: {e}")))?;
    if raw.len() != KEK_LEN {
        return Err(OffsiteBackupError::InvalidKek(format!(
            "master key length {} != {KEK_LEN}",
            raw.len()
        )));
    }
    let mut out = [0u8; KEK_LEN];
    out.copy_from_slice(&raw);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FINGERPRINT: &str = "deadbeefdeadbeefdeadbeefdeadbeef";

    fn kek(passphrase: &[u8]) -> [u8; KEK_LEN] {
        derive_kek_with_params(passphrase, FINGERPRINT, 512, 1, 1).unwrap()
    }

    #[test]
    fn kek_derive_roundtrip() {
        let kek = kek(b"correct horse battery staple");
        let stored = encrypt_payload(&kek, "AKIA...:secret").unwrap();
        let pt = decrypt_payload(&kek, &stored).unwrap();
        assert_eq!(pt, "AKIA...:secret");
    }

    #[test]
    fn passphrase_mismatch_derives_different_kek() {
        let a = kek(b"passphrase-one");
        let b = kek(b"passphrase-two");
        assert_ne!(a, b);
    }

    #[test]
    fn empty_passphrase_rejected() {
        let err = derive_kek_with_params(b"", FINGERPRINT, 512, 1, 1).expect_err("must reject");
        assert!(matches!(err, OffsiteBackupError::InvalidSecret(_)));
    }

    #[test]
    fn tampered_ciphertext_detected() {
        let kek = kek(b"correct horse battery staple");
        let mut stored = encrypt_payload(&kek, "super-secret").unwrap();
        let last = stored.len() - 1;
        let mut bytes = stored.into_bytes();
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        stored = String::from_utf8(bytes).unwrap();
        let err = decrypt_payload(&kek, &stored).expect_err("must detect tamper");
        assert_eq!(err, OffsiteBackupError::CredentialDecrypt);
    }

    #[test]
    fn wrap_unwrap_roundtrip() {
        let master = [0x42u8; KEK_LEN];
        let kek = kek(b"passphrase");
        let wrapped = wrap_kek(&master, &kek).unwrap();
        let unwrapped = unwrap_kek(&master, &wrapped).unwrap();
        assert_eq!(unwrapped, kek);
    }

    #[test]
    fn wrap_tamper_detected() {
        let master = [0x42u8; KEK_LEN];
        let kek = kek(b"passphrase");
        let mut wrapped = wrap_kek(&master, &kek).unwrap();
        let last = wrapped.len() - 1;
        let mut bytes = wrapped.into_bytes();
        bytes[last] = if bytes[last] == b'0' { b'1' } else { b'0' };
        wrapped = String::from_utf8(bytes).unwrap();
        assert!(matches!(
            unwrap_kek(&master, &wrapped),
            Err(OffsiteBackupError::InvalidKek(_))
        ));
    }

    #[test]
    fn master_key_fingerprint_stable() {
        let master = [0x42u8; KEK_LEN];
        assert_eq!(
            master_key_fingerprint(&master),
            master_key_fingerprint(&master)
        );
    }

    #[test]
    fn decode_master_key_roundtrip() {
        let master = [0x42u8; KEK_LEN];
        let encoded = B64.encode(master);
        assert_eq!(decode_master_key(&encoded).unwrap(), master);
        assert!(decode_master_key("").is_err());
        assert!(decode_master_key(&B64.encode([1u8; 16])).is_err());
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    const FINGERPRINT: &str = "cafe0123cafe0123cafe0123cafe0123";

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        #[test]
        fn encrypted_blob_roundtrips(p in "[a-zA-Z0-9:/._-]{0,256}") {
            let kek = derive_kek_with_params(b"passphrase", FINGERPRINT, 512, 1, 1).unwrap();
            let stored = encrypt_payload(&kek, &p).unwrap();
            prop_assert_eq!(decrypt_payload(&kek, &stored).unwrap(), p);
        }

        #[test]
        fn tampered_ciphertext_always_detected(byte in 0usize..512, p in "[a-zA-Z0-9]{1,128}") {
            let kek = derive_kek_with_params(b"passphrase", FINGERPRINT, 512, 1, 1).unwrap();
            let stored = encrypt_payload(&kek, &p).unwrap();
            let mut bytes = stored.into_bytes();
            if byte < bytes.len() {
                bytes[byte] ^= 0x01;
                let tampered = String::from_utf8(bytes).unwrap();
                prop_assert_eq!(decrypt_payload(&kek, &tampered), Err(OffsiteBackupError::CredentialDecrypt));
            }
        }
    }
}
