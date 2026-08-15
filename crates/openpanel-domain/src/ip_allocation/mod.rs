//! IPv6 + address-pool bounded context: typed pools, allocations
//! to sites, and the vhost binder that attaches the address set
//! to a vhost.
//!
//! Every allocation MUST lie inside its pool CIDR; a dedicated
//! allocation MUST never be assigned to two sites. The allocator
//! is pure — the integration tests build the SQLite store and
//! assert the rules end-to-end.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the IP allocation bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IpError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The CIDR is malformed.
    #[error("invalid CIDR: {0}")]
    InvalidCidr(String),
    /// The pool is exhausted.
    #[error("pool exhausted")]
    PoolExhausted,
    /// The address is already in use by another allocation.
    #[error("address in use: {0}")]
    AddressInUse(String),
    /// The address is outside the pool CIDR.
    #[error("address outside pool: {0}")]
    OutsidePool(String),
    /// The pool or allocation does not exist.
    #[error("not found: {0}")]
    NotFound(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<IpError> for RepoError {
    fn from(error: IpError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for IpError {
    fn from(error: RepoError) -> Self {
        IpError::Persistence(error.0)
    }
}

/// IP family of a pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpFamily {
    /// IPv4.
    V4,
    /// IPv6.
    V6,
}

impl IpFamily {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::V4 => "v4",
            Self::V6 => "v6",
        }
    }
}

/// Whether the pool is shared (any site may draw) or dedicated
/// (one allocation per site, never auto-shared).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolKind {
    /// Shared pool — the next-free address can be handed to any
    /// site that requests it.
    Shared,
    /// Dedicated pool — every address is reserved for a single
    /// site.
    Dedicated,
}

impl PoolKind {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Dedicated => "dedicated",
        }
    }
}

/// Status of an allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IpStatus {
    /// Reserved for a site but not yet bound to a vhost.
    Reserved,
    /// Bound to a vhost (in use).
    Active,
    /// Deallocated and released back to the pool.
    Released,
}

impl IpStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Reserved => "reserved",
            Self::Active => "active",
            Self::Released => "released",
        }
    }
}

/// An address pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpPool {
    /// Stable id.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Pool kind.
    pub kind: PoolKind,
    /// IP family.
    pub family: IpFamily,
    /// CIDR covered by the pool (e.g. `192.0.2.0/24`).
    pub cidr: String,
    /// When the pool was created.
    pub created_at: DateTime<Utc>,
}

impl IpPool {
    /// Validate the CIDR is well-formed.
    pub fn validate(&self) -> Result<(), IpError> {
        validate_cidr(&self.cidr, self.family)
    }
}

/// A single allocation within a pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpAllocation {
    /// Stable id.
    pub id: Uuid,
    /// Owning pool.
    pub pool_id: Uuid,
    /// Allocated address (within the pool CIDR).
    pub address: String,
    /// Owning site.
    pub site_id: Uuid,
    /// Current status.
    pub status: IpStatus,
    /// When the allocation was created.
    pub allocated_at: DateTime<Utc>,
    /// When the allocation was last bound to a vhost.
    #[serde(default)]
    pub bound_at: Option<DateTime<Utc>>,
}

/// A site's full address set (v4 + v6 + aliases).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteAddress {
    /// Site id.
    pub site_id: Uuid,
    /// IPv4 addresses.
    #[serde(default)]
    pub v4: Vec<String>,
    /// IPv6 addresses.
    #[serde(default)]
    pub v6: Vec<String>,
    /// When the vhost was last rebound.
    pub last_rebound_at: DateTime<Utc>,
}

/// Persistence port for the IP allocation bounded context.
#[async_trait]
pub trait IpRepository: Send + Sync + 'static {
    /// Persist a pool.
    async fn save_pool(&self, pool: &IpPool) -> Result<(), RepoError>;
    /// List all pools.
    async fn list_pools(&self) -> Result<Vec<IpPool>, RepoError>;
    /// Load a pool by id.
    async fn get_pool(&self, id: Uuid) -> Result<Option<IpPool>, RepoError>;
    /// Delete a pool.
    async fn delete_pool(&self, id: Uuid) -> Result<(), RepoError>;

    /// Persist an allocation.
    async fn save_allocation(&self, allocation: &IpAllocation) -> Result<(), RepoError>;
    /// List allocations for a site.
    async fn list_allocations_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<IpAllocation>, RepoError>;
    /// List allocations for a pool.
    async fn list_allocations_for_pool(
        &self,
        pool_id: Uuid,
    ) -> Result<Vec<IpAllocation>, RepoError>;
    /// Look up an allocation by address (within a pool).
    async fn get_allocation_by_address(
        &self,
        pool_id: Uuid,
        address: &str,
    ) -> Result<Option<IpAllocation>, RepoError>;
}

/// Validate a CIDR for a given family. The check rejects any
/// shape that doesn't match `addr/prefix` where `addr` is in the
/// expected family and `prefix` is a valid prefix length.
pub fn validate_cidr(cidr: &str, family: IpFamily) -> Result<(), IpError> {
    let (addr, prefix) = cidr
        .split_once('/')
        .ok_or_else(|| IpError::InvalidCidr("missing /".into()))?;
    let prefix: u8 = prefix
        .parse()
        .map_err(|_| IpError::InvalidCidr("prefix not u8".into()))?;
    let ok = match family {
        IpFamily::V4 => {
            // 0..=32 prefix; address has 4 octets separated by `.`
            prefix <= 32
                && addr
                    .split('.')
                    .filter(|p| !p.is_empty())
                    .count()
                    == 4
                && addr
                    .split('.')
                    .all(|p| p.parse::<u8>().is_ok())
        }
        IpFamily::V6 => {
            // 0..=128 prefix; address contains at least one `:` and
            // has only hex / `:` characters.
            prefix <= 128
                && addr.contains(':')
                && addr
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || c == ':')
        }
    };
    if !ok {
        return Err(IpError::InvalidCidr(cidr.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v4_cidr_accepts_and_rejects() {
        assert!(validate_cidr("192.0.2.0/24", IpFamily::V4).is_ok());
        assert!(validate_cidr("10.0.0.0/8", IpFamily::V4).is_ok());
        assert!(validate_cidr("2001:db8::/32", IpFamily::V4).is_err());
        assert!(validate_cidr("10.0.0.0/40", IpFamily::V4).is_err());
        assert!(validate_cidr("not-a-cidr", IpFamily::V4).is_err());
    }

    #[test]
    fn v6_cidr_accepts_and_rejects() {
        assert!(validate_cidr("2001:db8::/32", IpFamily::V6).is_ok());
        assert!(validate_cidr("fe80::/10", IpFamily::V6).is_ok());
        assert!(validate_cidr("192.0.2.0/24", IpFamily::V6).is_err());
        assert!(validate_cidr("2001:db8::/200", IpFamily::V6).is_err());
    }
}
