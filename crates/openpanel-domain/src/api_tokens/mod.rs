//! Scoped personal API-token domain model.

use std::{
    collections::BTreeSet,
    fmt,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

const PREFIX: &str = "openpanel_pat_";
const BODY_BYTES: usize = 15;
const BODY_CHARS: usize = 24;
const CHECKSUM_CHARS: usize = 4;
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// API-token validation or lifecycle failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ApiTokenError {
    /// Credential syntax or checksum is invalid.
    #[error("invalid API token")]
    InvalidCredential,
    /// Scope does not follow context:verb.
    #[error("invalid token scope")]
    InvalidScope,
    /// CIDR syntax or prefix length is invalid.
    #[error("invalid CIDR")]
    InvalidCidr,
    /// Token label is empty or too long.
    #[error("invalid token label")]
    InvalidLabel,
    /// Token must contain at least one scope.
    #[error("token requires at least one scope")]
    EmptyScopes,
    /// Expiration must be in the future.
    #[error("token expiration must be in the future")]
    InvalidExpiration,
    /// Revoked tokens cannot be revoked twice.
    #[error("token is already revoked")]
    AlreadyRevoked,
    /// Requested token was not found.
    #[error("API token not found")]
    NotFound,
    /// Actor is not allowed to issue or mutate this token.
    #[error("API token operation forbidden")]
    Forbidden,
    /// Persistence adapter failed.
    #[error("API token persistence failed: {0}")]
    Persistence(String),
}

/// Validated bounded-context and verb permission.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TokenScope(String);

impl TokenScope {
    /// Parse a lower-case scope containing exactly one colon.
    pub fn parse(value: &str) -> Result<Self, ApiTokenError> {
        let mut parts = value.split(':');
        let context = parts.next().unwrap_or_default();
        let verb = parts.next().unwrap_or_default();
        if parts.next().is_some() || !valid_scope_part(context) || !valid_scope_part(verb) {
            return Err(ApiTokenError::InvalidScope);
        }
        Ok(Self(value.to_owned()))
    }

    /// Bounded-context portion.
    pub fn context(&self) -> &str {
        self.0.split_once(':').map_or("", |parts| parts.0)
    }

    /// Verb portion.
    pub fn verb(&self) -> &str {
        self.0.split_once(':').map_or("", |parts| parts.1)
    }
}

impl fmt::Display for TokenScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

fn valid_scope_part(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.as_bytes()[0].is_ascii_lowercase()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

/// Validated IP network used by a token allowlist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Cidr(String);

impl Cidr {
    /// Parse and normalize a CIDR network.
    pub fn parse(value: &str) -> Result<Self, ApiTokenError> {
        let (address, prefix) = value.split_once('/').ok_or(ApiTokenError::InvalidCidr)?;
        let address: IpAddr = address.parse().map_err(|_| ApiTokenError::InvalidCidr)?;
        let prefix: u8 = prefix.parse().map_err(|_| ApiTokenError::InvalidCidr)?;
        let max = if address.is_ipv4() { 32 } else { 128 };
        if prefix > max {
            return Err(ApiTokenError::InvalidCidr);
        }
        Ok(Self(format!("{address}/{prefix}")))
    }

    /// Whether the address is contained by this prefix.
    pub fn contains(&self, candidate: IpAddr) -> bool {
        let Some((address, prefix)) = self.0.split_once('/') else {
            return false;
        };
        let Ok(network) = address.parse::<IpAddr>() else {
            return false;
        };
        let Ok(prefix) = prefix.parse::<u8>() else {
            return false;
        };
        match (network, candidate) {
            (IpAddr::V4(network), IpAddr::V4(candidate)) => {
                prefix_match_v4(network, candidate, prefix)
            }
            (IpAddr::V6(network), IpAddr::V6(candidate)) => {
                prefix_match_v6(network, candidate, prefix)
            }
            _ => false,
        }
    }

    /// Normalized CIDR text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn prefix_match_v4(network: Ipv4Addr, candidate: Ipv4Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    u32::from(network) & mask == u32::from(candidate) & mask
}

fn prefix_match_v6(network: Ipv6Addr, candidate: Ipv6Addr, prefix: u8) -> bool {
    let mask = if prefix == 0 {
        0
    } else {
        u128::MAX << (128 - prefix)
    };
    u128::from(network) & mask == u128::from(candidate) & mask
}

/// HMAC-SHA256 digest persisted instead of plaintext token material.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenHash([u8; 32]);

impl TokenHash {
    /// Restore a hash from its database bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Hash bytes for persistence and constant-time lookup.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex representation useful for non-secret diagnostics and tests.
    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Constant-time equality check.
    pub fn constant_time_eq(&self, other: &Self) -> bool {
        self.0
            .iter()
            .zip(other.0.iter())
            .fold(0u8, |difference, (left, right)| difference | (left ^ right))
            == 0
    }
}

impl fmt::Debug for TokenHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenHash([REDACTED])")
    }
}

/// Parsed plaintext credential retained only for a create or rotate response.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenCredential {
    rendered: String,
    body: String,
}

impl TokenCredential {
    /// Generate a 120-bit credential with a typo-detecting checksum.
    pub fn generate() -> Self {
        let mut bytes = [0u8; BODY_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        let body = base32_encode(&bytes);
        let checksum = checksum(&body);
        Self {
            rendered: format!("{PREFIX}{body}.{checksum}"),
            body,
        }
    }

    /// Parse and validate an OpenPanel personal access token.
    pub fn parse(value: &str) -> Result<Self, ApiTokenError> {
        let suffix = value
            .strip_prefix(PREFIX)
            .ok_or(ApiTokenError::InvalidCredential)?;
        let (body, supplied_checksum) = suffix
            .split_once('.')
            .ok_or(ApiTokenError::InvalidCredential)?;
        if body.len() != BODY_CHARS
            || supplied_checksum.len() != CHECKSUM_CHARS
            || base32_decode(body).is_none()
            || !constant_time_text_eq(supplied_checksum, &checksum(body))
        {
            return Err(ApiTokenError::InvalidCredential);
        }
        Ok(Self {
            rendered: value.to_owned(),
            body: body.to_owned(),
        })
    }

    /// Plaintext credential. Callers must show it once and never persist it.
    pub fn expose(&self) -> &str {
        &self.rendered
    }

    /// Random credential body used as HMAC input.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Derive the database lookup hash under the panel pepper.
    pub fn hash(&self, pepper: &[u8; 32]) -> TokenHash {
        #[allow(clippy::expect_used)]
        let mut mac = Hmac::<Sha256>::new_from_slice(pepper)
            .expect("HMAC-SHA256 accepts every fixed-size 32-byte pepper");
        mac.update(self.body.as_bytes());
        TokenHash(mac.finalize().into_bytes().into())
    }
}

impl fmt::Debug for TokenCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("TokenCredential([REDACTED])")
    }
}

fn checksum(body: &str) -> String {
    let digest = Sha256::digest(body.as_bytes());
    base32_encode(&digest[..3])[..CHECKSUM_CHARS].to_owned()
}

fn constant_time_text_eq(left: &str, right: &str) -> bool {
    left.len() == right.len()
        && left
            .bytes()
            .zip(right.bytes())
            .fold(0u8, |difference, (a, b)| difference | (a ^ b))
            == 0
}

fn base32_encode(bytes: &[u8]) -> String {
    let mut result = String::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(*byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            result.push(char::from(ALPHABET[((buffer >> bits) & 31) as usize]));
        }
    }
    if bits > 0 {
        result.push(char::from(ALPHABET[((buffer << (5 - bits)) & 31) as usize]));
    }
    result
}

fn base32_decode(value: &str) -> Option<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = 0u32;
    let mut bits = 0u8;
    for byte in value.bytes() {
        let index = ALPHABET.iter().position(|candidate| *candidate == byte)? as u32;
        buffer = (buffer << 5) | index;
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            result.push(((buffer >> bits) & 0xff) as u8);
        }
    }
    Some(result)
}

/// Persisted API-token aggregate.
#[derive(Debug, Clone)]
pub struct ApiToken {
    id: Uuid,
    user_id: Uuid,
    label: String,
    hash: TokenHash,
    scopes: BTreeSet<TokenScope>,
    cidr_allowlist: Vec<Cidr>,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    last_used_at: Option<DateTime<Utc>>,
    revoked_at: Option<DateTime<Utc>>,
}

/// Public token metadata. Credential plaintext and hash are deliberately absent.
#[derive(Debug, Clone, Serialize)]
pub struct ApiTokenMetadata {
    /// Token identifier.
    pub id: Uuid,
    /// Principal identifier.
    pub user_id: Uuid,
    /// Caller-provided display label.
    pub label: String,
    /// Granted scopes.
    pub scopes: BTreeSet<TokenScope>,
    /// Optional source network allowlist.
    pub cidr_allowlist: Vec<Cidr>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Expiration time.
    pub expires_at: DateTime<Utc>,
    /// Last successful bearer use.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Revocation time.
    pub revoked_at: Option<DateTime<Utc>>,
}

impl ApiToken {
    /// Construct a new active token.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        user_id: Uuid,
        label: String,
        hash: TokenHash,
        scopes: BTreeSet<TokenScope>,
        cidr_allowlist: Vec<Cidr>,
        expires_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, ApiTokenError> {
        Self::restore(
            id,
            user_id,
            label,
            hash,
            scopes,
            cidr_allowlist,
            now,
            expires_at,
            None,
            None,
        )
    }

    /// Restore a token from trusted persistence fields.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        user_id: Uuid,
        label: String,
        hash: TokenHash,
        scopes: BTreeSet<TokenScope>,
        cidr_allowlist: Vec<Cidr>,
        created_at: DateTime<Utc>,
        expires_at: DateTime<Utc>,
        last_used_at: Option<DateTime<Utc>>,
        revoked_at: Option<DateTime<Utc>>,
    ) -> Result<Self, ApiTokenError> {
        if label.trim().is_empty() || label.len() > 120 {
            return Err(ApiTokenError::InvalidLabel);
        }
        if scopes.is_empty() {
            return Err(ApiTokenError::EmptyScopes);
        }
        if expires_at <= created_at {
            return Err(ApiTokenError::InvalidExpiration);
        }
        Ok(Self {
            id,
            user_id,
            label: label.trim().to_owned(),
            hash,
            scopes,
            cidr_allowlist,
            created_at,
            expires_at,
            last_used_at,
            revoked_at,
        })
    }

    /// Revoke an active token immediately.
    pub fn revoke(&mut self, at: DateTime<Utc>) -> Result<(), ApiTokenError> {
        if self.revoked_at.is_some() {
            return Err(ApiTokenError::AlreadyRevoked);
        }
        self.revoked_at = Some(at);
        Ok(())
    }

    /// Record successful bearer use.
    pub fn mark_used(&mut self, at: DateTime<Utc>) {
        self.last_used_at = Some(at);
    }

    /// Whether the token can authenticate at now.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && now < self.expires_at
    }

    /// Whether a source address is allowed.
    pub fn allows_ip(&self, address: IpAddr) -> bool {
        self.cidr_allowlist.is_empty()
            || self
                .cidr_allowlist
                .iter()
                .any(|cidr| cidr.contains(address))
    }

    /// Whether the exact requested scope is granted.
    pub fn allows_scope(&self, scope: &TokenScope) -> bool {
        self.scopes.contains(scope)
    }

    /// Safe response metadata.
    pub fn metadata(&self) -> ApiTokenMetadata {
        ApiTokenMetadata {
            id: self.id,
            user_id: self.user_id,
            label: self.label.clone(),
            scopes: self.scopes.clone(),
            cidr_allowlist: self.cidr_allowlist.clone(),
            created_at: self.created_at,
            expires_at: self.expires_at,
            last_used_at: self.last_used_at,
            revoked_at: self.revoked_at,
        }
    }

    /// Token identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owning principal identifier.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Display label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Lookup hash.
    pub fn hash(&self) -> &TokenHash {
        &self.hash
    }

    /// Granted scopes.
    pub fn scopes(&self) -> &BTreeSet<TokenScope> {
        &self.scopes
    }

    /// Source allowlist.
    pub fn cidr_allowlist(&self) -> &[Cidr] {
        &self.cidr_allowlist
    }

    /// Creation time.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Expiration time.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Last successful use.
    pub fn last_used_at(&self) -> Option<DateTime<Utc>> {
        self.last_used_at
    }

    /// Revocation time.
    pub fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
}

/// Persistence port for the API-token bounded context.
#[async_trait]
pub trait ApiTokenRepository: Send + Sync {
    /// Insert a token.
    async fn create(&self, token: &ApiToken) -> Result<(), RepoError>;
    /// Persist mutable state or rotated hash.
    async fn update(&self, token: &ApiToken) -> Result<(), RepoError>;
    /// Atomically revoke old and insert new.
    async fn rotate(&self, old: &ApiToken, new: &ApiToken) -> Result<(), RepoError>;
    /// Find by owner and id.
    async fn find(&self, user_id: Uuid, id: Uuid) -> Result<Option<ApiToken>, RepoError>;
    /// Find by HMAC hash.
    async fn find_by_hash(&self, hash: &TokenHash) -> Result<Option<ApiToken>, RepoError>;
    /// List metadata for an owner.
    async fn list(&self, user_id: Uuid) -> Result<Vec<ApiToken>, RepoError>;
}
