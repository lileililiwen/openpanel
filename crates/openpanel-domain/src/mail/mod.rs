//! Hosted-mail value objects and safety invariants.

use std::{collections::HashMap, fmt};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Mail-domain validation failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MailError {
    /// Input is invalid or outside configured bounds.
    #[error("invalid mail value")]
    Invalid,
    /// An alias would create a direct or transitive cycle.
    #[error("mail alias cycle")]
    AliasCycle,
}

/// Canonical ASCII mail domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MailDomainName(String);
impl MailDomainName {
    /// Parse and canonicalize an ASCII domain.
    pub fn new(value: impl AsRef<str>) -> Result<Self, MailError> {
        let value = value
            .as_ref()
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        if value.is_empty() || value.len() > 253 || !value.is_ascii() {
            return Err(MailError::Invalid);
        }
        if !value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        }) {
            return Err(MailError::Invalid);
        }
        Ok(Self(value))
    }

    /// Canonical domain.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl fmt::Display for MailDomainName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Canonical ASCII mailbox address.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MailAddress {
    local: String,
    domain: MailDomainName,
    address: String,
}
impl MailAddress {
    /// Construct from a local part and validated domain.
    pub fn new(local: impl AsRef<str>, domain: MailDomainName) -> Result<Self, MailError> {
        let local = local.as_ref().trim().to_ascii_lowercase();
        if local.is_empty()
            || local.len() > 64
            || !local.is_ascii()
            || local.starts_with('.')
            || local.ends_with('.')
            || local.contains("..")
            || !local
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
        {
            return Err(MailError::Invalid);
        }
        let address = format!("{local}@{domain}");
        Ok(Self {
            local,
            domain,
            address,
        })
    }

    /// Parse a complete address.
    pub fn parse(value: &str) -> Result<Self, MailError> {
        let (local, domain) = value.rsplit_once('@').ok_or(MailError::Invalid)?;
        Self::new(local, MailDomainName::new(domain)?)
    }

    /// Canonical address.
    pub fn as_str(&self) -> &str {
        &self.address
    }

    /// Address domain.
    pub fn domain(&self) -> &MailDomainName {
        &self.domain
    }

    /// Canonical local part.
    pub fn local(&self) -> &str {
        &self.local
    }
}
impl fmt::Display for MailAddress {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.local, self.domain)
    }
}

/// Validated storage quota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailQuota(u64);
impl MailQuota {
    /// Validate a quota against installation bounds.
    pub fn new(value: u64, minimum: u64, maximum: u64) -> Result<Self, MailError> {
        if minimum == 0 || minimum > maximum || value < minimum || value > maximum {
            return Err(MailError::Invalid);
        }
        Ok(Self(value))
    }

    /// Quota bytes.
    pub fn bytes(self) -> u64 {
        self.0
    }

    /// Whether accepting a delivery would exceed quota.
    pub fn permits(self, used: u64, incoming: u64) -> bool {
        used.checked_add(incoming)
            .is_some_and(|total| total <= self.0)
    }
}

/// Validated outbound message rate for a fixed window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutboundRateLimit {
    maximum: u32,
    window_seconds: u32,
}
impl OutboundRateLimit {
    /// Construct a bounded limit (one to 10,000 messages per 1–1,440 minutes).
    pub fn new(maximum: u32, window_seconds: u32) -> Result<Self, MailError> {
        if maximum == 0 || maximum > 10_000 || !(60..=86_400).contains(&window_seconds) {
            return Err(MailError::Invalid);
        }
        Ok(Self {
            maximum,
            window_seconds,
        })
    }

    /// Whether another message may be accepted in the current window.
    pub fn permits(self, sent: u32) -> bool {
        sent < self.maximum
    }

    /// Window length in seconds.
    pub fn window_seconds(self) -> u32 {
        self.window_seconds
    }
}

/// Directed alias graph with cycle prevention.
#[derive(Debug, Default, Clone)]
pub struct AliasGraph {
    edges: HashMap<MailAddress, MailAddress>,
}
impl AliasGraph {
    /// Add one forwarder if it cannot become cyclic.
    pub fn add(&mut self, source: MailAddress, destination: MailAddress) -> Result<(), MailError> {
        if source == destination {
            return Err(MailError::AliasCycle);
        }
        let mut cursor = destination.clone();
        for _ in 0..=self.edges.len() {
            if cursor == source {
                return Err(MailError::AliasCycle);
            }
            match self.edges.get(&cursor) {
                Some(next) => cursor = next.clone(),
                None => {
                    self.edges.insert(source, destination);
                    return Ok(());
                }
            }
        }
        Err(MailError::AliasCycle)
    }
}

/// Default-deny relay policy.
pub struct RelayPolicy {
    local_domains: Vec<MailDomainName>,
}
impl RelayPolicy {
    /// Construct from managed local domains.
    pub fn new(local_domains: Vec<MailDomainName>) -> Self {
        Self { local_domains }
    }

    /// Authenticated clients may relay; unauthenticated clients may deliver only locally.
    pub fn allows(
        &self,
        authenticated: bool,
        _sender: &MailAddress,
        recipient: &MailAddress,
    ) -> bool {
        authenticated || self.local_domains.contains(recipient.domain())
    }
}

/// Short-lived destructive confirmation scoped to one domain.
#[derive(Debug, Clone)]
pub struct DeletionToken {
    target: String,
    expires_at: u64,
}
impl DeletionToken {
    /// Issue a token with a positive bounded lifetime.
    pub fn issue(target: impl Into<String>, now: u64, ttl: u64) -> Result<Self, MailError> {
        let target = target.into();
        if target.is_empty() || ttl == 0 || ttl > 300 {
            return Err(MailError::Invalid);
        }
        Ok(Self {
            target,
            expires_at: now.checked_add(ttl).ok_or(MailError::Invalid)?,
        })
    }

    /// Verify scope and expiration.
    pub fn verify(&self, target: &str, now: u64) -> bool {
        self.target == target && now <= self.expires_at
    }
}
