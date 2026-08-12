//! Two-factor authentication: factor model, TOTP value object, and
//! recovery codes.

use std::fmt;

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use totp_rs::{Algorithm, Secret, TOTP};
use uuid::Uuid;

/// Factor error variants.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum FactorError {
    /// The factor has been revoked and cannot be used.
    #[error("factor has been revoked")]
    Revoked,
    /// The factor has not been verified yet (enrollment incomplete).
    #[error("factor is not yet verified")]
    NotVerified,
    /// The TOTP code is malformed (e.g. not 6 digits).
    #[error("invalid TOTP code format")]
    InvalidCodeFormat,
    /// The TOTP code did not match.
    #[error("TOTP code did not match")]
    CodeMismatch,
    /// The TOTP code was already used (replay protection).
    #[error("TOTP code has already been used")]
    CodeReplayed,
    /// Recovery code did not match any stored hash.
    #[error("recovery code did not match")]
    RecoveryMismatch,
    /// Recovery code was already consumed.
    #[error("recovery code has already been consumed")]
    RecoveryConsumed,
    /// TOTP secret bytes could not be decoded.
    #[error("TOTP secret is malformed")]
    InvalidSecret,
}

/// The kind of second-factor credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactorKind {
    /// Time-based One-Time Password (RFC 6238).
    Totp,
    /// WebAuthn / FIDO2 authenticator. Phase B.
    #[serde(other)]
    WebAuthn,
}

impl FactorKind {
    /// Stable, snake-case string for storage and audit.
    pub fn as_str(self) -> &'static str {
        match self {
            FactorKind::Totp => "totp",
            FactorKind::WebAuthn => "webauthn",
        }
    }

    /// Parse from a stable string.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "totp" => Some(FactorKind::Totp),
            "webauthn" => Some(FactorKind::WebAuthn),
            _ => None,
        }
    }
}

impl fmt::Display for FactorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A second-factor credential enrolled against a user account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Factor {
    id: Uuid,
    user_id: Uuid,
    kind: FactorKind,
    created_at: DateTime<Utc>,
    verified_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
    /// Last accepted TOTP step (Unix seconds / 30). `None` before first
    /// verification. Only meaningful for TOTP factors.
    last_used_step: Option<i64>,
}

impl Factor {
    /// Construct a fresh, just-verified factor (e.g. a TOTP whose
    /// verification code just succeeded).
    pub fn new_verified(id: Uuid, user_id: Uuid, kind: FactorKind, now: DateTime<Utc>) -> Self {
        Self {
            id,
            user_id,
            kind,
            created_at: now,
            verified_at: now,
            last_used_at: None,
            revoked_at: None,
            last_used_step: None,
        }
    }

    /// Reconstruct from storage.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        user_id: Uuid,
        kind: FactorKind,
        created_at: DateTime<Utc>,
        verified_at: DateTime<Utc>,
        last_used_at: Option<DateTime<Utc>>,
        revoked_at: Option<DateTime<Utc>>,
        last_used_step: Option<i64>,
    ) -> Self {
        Self {
            id,
            user_id,
            kind,
            created_at,
            verified_at,
            last_used_at,
            revoked_at,
            last_used_step,
        }
    }

    /// Whether this factor is currently usable.
    pub fn is_usable(&self) -> bool {
        self.revoked_at.is_none()
    }

    /// Mark the factor as used at the given step. Returns the new step
    /// stored. Rejects if `step` is not strictly greater than the
    /// previously stored step (replay protection).
    pub fn record_used_step(&mut self, step: i64) -> Result<(), FactorError> {
        if !self.is_usable() {
            return Err(FactorError::Revoked);
        }
        if let Some(prev) = self.last_used_step
            && step <= prev
        {
            return Err(FactorError::CodeReplayed);
        }
        self.last_used_step = Some(step);
        self.last_used_at = Some(Utc::now());
        Ok(())
    }

    /// Mark the factor as revoked.
    pub fn revoke(&mut self, now: DateTime<Utc>) {
        self.revoked_at = Some(now);
    }

    /// Stable identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owner user id.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Factor kind.
    pub fn kind(&self) -> FactorKind {
        self.kind
    }

    /// Created timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Verified timestamp.
    pub fn verified_at(&self) -> DateTime<Utc> {
        self.verified_at
    }

    /// Last-used timestamp, if any.
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    /// Revoked timestamp, if any.
    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }

    /// TOTP step last accepted, if any.
    pub fn last_used_step(&self) -> Option<i64> {
        self.last_used_step
    }
}

/// TOTP secret and per-factor configuration.
///
/// Stored encrypted at rest; the plaintext lives only in this value
/// object and the otpauth URI shown at enrollment.
#[derive(Debug, Clone)]
pub struct TotpSecret {
    bytes: Vec<u8>,
    digits: usize,
    period: u64,
    algorithm: Algorithm,
}

impl TotpSecret {
    /// Generate a fresh 160-bit (20-byte) secret and the RFC 6238
    /// defaults (6 digits, 30-second period, SHA-1).
    pub fn generate() -> Self {
        // generate_secret always decodes; fallback Vec is unreachable in practice.
        let bytes = Secret::generate_secret().to_bytes().unwrap_or_default();
        Self {
            bytes,
            digits: 6,
            period: 30,
            algorithm: Algorithm::SHA1,
        }
    }

    /// Reconstruct from stored raw bytes.
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, FactorError> {
        if bytes.is_empty() {
            return Err(FactorError::InvalidSecret);
        }
        Ok(Self {
            bytes,
            digits: 6,
            period: 30,
            algorithm: Algorithm::SHA1,
        })
    }

    /// Raw secret bytes (for storage encryption).
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Base32-encoded secret (for QR / manual entry).
    pub fn to_base32(&self) -> String {
        Secret::Encoded(Self::encode_base32(&self.bytes)).to_string()
    }

    /// Build the otpauth:// URI for provisioning into authenticator apps.
    pub fn provisioning_uri(&self, issuer: &str, account_name: &str) -> String {
        let totp = self.totp(issuer, account_name);
        totp.get_url()
    }

    /// Build the underlying TOTP calculator (no issuer/account).
    pub(crate) fn totp(&self, issuer: &str, account_name: &str) -> TOTP {
        match TOTP::new(
            self.algorithm,
            self.digits,
            1,
            self.period,
            self.bytes.clone(),
            Some(issuer.to_owned()),
            account_name.to_owned(),
        ) {
            Ok(totp) => totp,
            Err(_) => TOTP::new_unchecked(
                self.algorithm,
                self.digits,
                1,
                self.period,
                self.bytes.clone(),
                Some(issuer.to_owned()),
                account_name.to_owned(),
            ),
        }
    }

    /// Current TOTP step (Unix seconds / 30).
    pub fn current_step(&self, now: DateTime<Utc>) -> i64 {
        now.timestamp() / self.period as i64
    }

    /// Verify a code against this secret with a ±1 step window,
    /// returning the step that matched (so the caller can advance
    /// replay-protection state).
    pub fn verify(&self, code: &str, now: DateTime<Utc>) -> Result<i64, FactorError> {
        let normalized = code.trim().replace(' ', "");
        if normalized.len() != self.digits || !normalized.bytes().all(|b| b.is_ascii_digit()) {
            return Err(FactorError::InvalidCodeFormat);
        }
        let current = self.current_step(now);
        for offset in &[0_i64, -1, 1] {
            let step = current + offset;
            let step_u = u64::try_from(step).map_err(|_| FactorError::InvalidSecret)?;
            let totp = self.totp("", "");
            let expected = totp.generate(step_u);
            if constant_time_eq(expected.as_bytes(), normalized.as_bytes()) {
                return Ok(step);
            }
        }
        Err(FactorError::CodeMismatch)
    }

    fn encode_base32(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
        let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
        let mut buffer: u64 = 0;
        let mut bits: u32 = 0;
        for byte in bytes {
            buffer = (buffer << 8) | u64::from(*byte);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                let idx = ((buffer >> bits) & 0x1F) as usize;
                out.push(ALPHABET[idx] as char);
            }
        }
        if bits > 0 {
            let idx = ((buffer << (5 - bits)) & 0x1F) as usize;
            out.push(ALPHABET[idx] as char);
        }
        out
    }
}

/// Constant-time equality on two byte slices. Used to defeat timing
/// side-channels when comparing TOTP codes.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// A single-use recovery code plus its argon2 hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryCode {
    /// Plaintext code (base32-Crockford, see [`generate_recovery_code`]).
    pub plaintext: String,
    /// Independent argon2id PHC hash of the plaintext.
    pub hash: String,
}

impl RecoveryCode {
    /// Compute the argon2id PHC hash of a plaintext code.
    pub fn hash(plaintext: &str) -> Result<String, FactorError> {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(plaintext.as_bytes(), &salt)
            .map_err(|_| FactorError::InvalidSecret)
            .map(|h| h.to_string())
    }

    /// Verify a presented plaintext against the stored hash.
    pub fn verify(plaintext: &str, hash: &str) -> Result<(), FactorError> {
        let parsed = PasswordHash::new(hash).map_err(|_| FactorError::RecoveryMismatch)?;
        Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .map_err(|_| FactorError::RecoveryMismatch)
    }
}

/// A user's set of recovery codes. Created at first factor enrollment;
/// regenerated on explicit user request.
#[derive(Debug, Clone)]
pub struct RecoveryCodeSet {
    user_id: Uuid,
    created_at: DateTime<Utc>,
    remaining: u8,
    /// Bounded list of stored hashes (one per code).
    entries: Vec<RecoveryCodeEntry>,
}

#[derive(Debug, Clone)]
struct RecoveryCodeEntry {
    /// argon2id PHC hash of the plaintext.
    hash: String,
    /// Unix-second timestamp the code was consumed (single-use).
    consumed_at: Option<DateTime<Utc>>,
}

impl RecoveryCodeSet {
    /// Generate a fresh set of `count` single-use recovery codes for the
    /// given user. The plaintexts are returned to the caller once and
    /// never persisted.
    pub fn generate(user_id: Uuid, count: u8, now: DateTime<Utc>) -> (Self, Vec<String>) {
        let mut plaintexts = Vec::with_capacity(count as usize);
        let mut entries = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let code = generate_recovery_code();
            // argon2id with a random salt never fails; the unwrap_or_default is unreachable.
            let hash = hash_recovery_code(&code).unwrap_or_default();
            plaintexts.push(code);
            entries.push(RecoveryCodeEntry {
                hash,
                consumed_at: None,
            });
        }
        let set = Self {
            user_id,
            created_at: now,
            remaining: count,
            entries,
        };
        (set, plaintexts)
    }

    /// Reconstruct from storage.
    pub fn restore(
        user_id: Uuid,
        created_at: DateTime<Utc>,
        entries: Vec<(String, Option<DateTime<Utc>>)>,
    ) -> Self {
        let remaining = entries
            .iter()
            .filter(|(_, consumed)| consumed.is_none())
            .count() as u8;
        Self {
            user_id,
            created_at,
            remaining,
            entries: entries
                .into_iter()
                .map(|(hash, consumed_at)| RecoveryCodeEntry { hash, consumed_at })
                .collect(),
        }
    }

    /// Look up a plaintext against the stored hashes and, on match,
    /// mark the entry as consumed. Returns the new remaining count.
    pub fn consume(&mut self, plaintext: &str, now: DateTime<Utc>) -> Result<u8, FactorError> {
        for entry in &mut self.entries {
            if entry.consumed_at.is_none() && RecoveryCode::verify(plaintext, &entry.hash).is_ok() {
                entry.consumed_at = Some(now);
                self.remaining = self.remaining.saturating_sub(1);
                return Ok(self.remaining);
            }
        }
        Err(FactorError::RecoveryMismatch)
    }

    /// User id.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Created timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Count of codes still available.
    pub fn remaining(&self) -> u8 {
        self.remaining
    }
}

/// Format a recovery code as a Crockford-base32 12-character group
/// (e.g. `ABCD-EFGH-JKLM`). The alphabet omits 0/O, 1/I/L to ease
/// manual transcription.
pub fn generate_recovery_code() -> String {
    let mut bytes = [0u8; 8];
    rand::Rng::fill(&mut OsRng, &mut bytes);
    let encoded = base32_crockford(&bytes);
    let chars: Vec<char> = encoded.chars().take(12).collect();
    let s: String = chars.into_iter().collect();
    format!("{}-{}-{}", &s[0..4], &s[4..8], &s[8..12])
}

fn hash_recovery_code(code: &str) -> Result<String, FactorError> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(code.as_bytes(), &salt)
        .map_err(|_| FactorError::InvalidSecret)?
        .to_string();
    Ok(hash)
}

fn base32_crockford(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let mut out = String::with_capacity((bytes.len() * 8).div_ceil(5));
    let mut buffer: u64 = 0;
    let mut bits: u32 = 0;
    for byte in bytes {
        buffer = (buffer << 8) | u64::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            let idx = ((buffer >> bits) & 0x1F) as usize;
            out.push(ALPHABET[idx] as char);
        }
    }
    if bits > 0 {
        let idx = ((buffer << (5 - bits)) & 0x1F) as usize;
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// Pending-login state created when a password authenticates a user
/// that has at least one enrolled factor. Single-use, time-limited,
/// stored hashed.
#[derive(Debug, Clone)]
pub struct TwoFactorChallenge {
    id: Uuid,
    user_id: Uuid,
    token_hash: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    consumed_at: Option<DateTime<Utc>>,
    source_ip: Option<String>,
    user_agent: Option<String>,
}

impl TwoFactorChallenge {
    /// Default challenge lifetime (5 minutes).
    pub const DEFAULT_LIFETIME: chrono::Duration = chrono::Duration::minutes(5);

    /// Create a new challenge for the given user. Returns the (challenge,
    /// plaintext token) pair; the plaintext is what the browser stores
    /// (cookie or hidden form field) and is not persisted.
    pub fn create(
        user_id: Uuid,
        source_ip: Option<String>,
        user_agent: Option<String>,
        now: DateTime<Utc>,
    ) -> (Self, String) {
        let id = Uuid::new_v4();
        let token = random_token();
        let token_hash = hash_token(&token);
        let challenge = Self {
            id,
            user_id,
            token_hash,
            created_at: now,
            expires_at: now + Self::DEFAULT_LIFETIME,
            consumed_at: None,
            source_ip,
            user_agent,
        };
        (challenge, token)
    }

    /// Reconstruct from storage.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        user_id: Uuid,
        token_hash: String,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        consumed_at: Option<DateTime<Utc>>,
        source_ip: Option<String>,
        user_agent: Option<String>,
    ) -> Self {
        Self {
            id,
            user_id,
            token_hash,
            created_at,
            expires_at,
            consumed_at,
            source_ip,
            user_agent,
        }
    }

    /// Whether this challenge can still be used. Expired or consumed
    /// challenges return false.
    pub fn is_usable(&self, now: DateTime<Utc>) -> bool {
        self.consumed_at.is_none() && now < self.expires_at
    }

    /// Mark the challenge as consumed. Returns the user id on success.
    pub fn consume(&mut self, now: DateTime<Utc>) -> Result<Uuid, FactorError> {
        if !self.is_usable(now) {
            return Err(FactorError::CodeReplayed);
        }
        self.consumed_at = Some(now);
        Ok(self.user_id)
    }

    /// Verify the presented plaintext against the stored hash.
    pub fn verify_token(&self, plaintext: &str) -> bool {
        let parsed = match PasswordHash::new(&self.token_hash) {
            Ok(parsed) => parsed,
            Err(_) => return false,
        };
        Argon2::default()
            .verify_password(plaintext.as_bytes(), &parsed)
            .is_ok()
    }

    /// Stable identifier returned to the browser.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Argon2id PHC hash of the plaintext token. The plaintext is held
    /// only in the browser and is never persisted.
    pub fn token_hash(&self) -> &str {
        &self.token_hash
    }

    /// Owner user id.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Created timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Expiry timestamp.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Consumed timestamp, if any.
    pub fn consumed_at(&self) -> Option<DateTime<Utc>> {
        self.consumed_at
    }

    /// Source IP captured at challenge creation.
    pub fn source_ip(&self) -> Option<&str> {
        self.source_ip.as_deref()
    }

    /// User-Agent captured at challenge creation.
    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }
}

fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::Rng::fill(&mut OsRng, &mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    // argon2id with a random salt never fails; the unwrap_or_default is unreachable.
    Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map(|h| h.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_verify_accepts_current_code_and_rejects_random() {
        let secret = TotpSecret::generate();
        let totp = secret.totp("OpenPanel", "test@example.com");
        let now = Utc::now();
        let step = secret.current_step(now);
        let step_u = u64::try_from(step).unwrap();
        let code = totp.generate(step_u);
        assert_eq!(secret.verify(&code, now).unwrap(), step);
        assert_eq!(
            secret.verify("000000", now).unwrap_err(),
            FactorError::CodeMismatch
        );
    }

    #[test]
    fn totp_verify_rejects_malformed_code() {
        let secret = TotpSecret::generate();
        let now = Utc::now();
        assert_eq!(
            secret.verify("abc", now).unwrap_err(),
            FactorError::InvalidCodeFormat
        );
        assert_eq!(
            secret.verify("12345", now).unwrap_err(),
            FactorError::InvalidCodeFormat
        );
    }

    #[test]
    fn factor_replay_protection_advances_last_used_step() {
        let mut factor =
            Factor::new_verified(Uuid::new_v4(), Uuid::new_v4(), FactorKind::Totp, Utc::now());
        factor.record_used_step(100).unwrap();
        assert_eq!(
            factor.record_used_step(100).unwrap_err(),
            FactorError::CodeReplayed
        );
        factor.record_used_step(101).unwrap();
    }

    #[test]
    fn factor_revoke_means_record_used_step_fails() {
        let mut factor =
            Factor::new_verified(Uuid::new_v4(), Uuid::new_v4(), FactorKind::Totp, Utc::now());
        factor.revoke(Utc::now());
        assert!(!factor.is_usable());
        assert_eq!(
            factor.record_used_step(1).unwrap_err(),
            FactorError::Revoked
        );
    }

    #[test]
    fn recovery_code_round_trip_and_consume() {
        let user_id = Uuid::new_v4();
        let (mut set, plaintexts) = RecoveryCodeSet::generate(user_id, 3, Utc::now());
        assert_eq!(set.remaining(), 3);
        let remaining = set.consume(&plaintexts[0], Utc::now()).unwrap();
        assert_eq!(remaining, 2);
        // Replay rejected.
        assert_eq!(
            set.consume(&plaintexts[0], Utc::now()).unwrap_err(),
            FactorError::RecoveryMismatch
        );
    }

    #[test]
    fn recovery_code_format_is_three_groups_of_four() {
        let code = generate_recovery_code();
        let parts: Vec<&str> = code.split('-').collect();
        assert_eq!(parts.len(), 3);
        for part in parts {
            assert_eq!(part.len(), 4);
            assert!(
                part.chars()
                    .all(|c| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(c))
            );
        }
    }

    #[test]
    fn challenge_is_single_use_and_time_limited() {
        let user_id = Uuid::new_v4();
        let now = Utc::now();
        let (mut challenge, token) = TwoFactorChallenge::create(user_id, None, None, now);
        assert!(challenge.is_usable(now));
        assert!(challenge.verify_token(&token));
        let user = challenge.consume(now).unwrap();
        assert_eq!(user, user_id);
        assert!(!challenge.is_usable(now));
        // Reject after expiry.
        let later = now + TwoFactorChallenge::DEFAULT_LIFETIME + chrono::Duration::seconds(1);
        let (mut expired, _) = TwoFactorChallenge::create(user_id, None, None, now);
        assert!(!expired.is_usable(later));
        assert_eq!(
            expired.consume(later).unwrap_err(),
            FactorError::CodeReplayed
        );
    }

    #[test]
    fn constant_time_eq_matches_byte_strings() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
    }
}
