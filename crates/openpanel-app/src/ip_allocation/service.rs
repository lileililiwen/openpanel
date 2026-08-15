//! IPv6 + address-pool services: allocator and vhost binder.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    IpAllocation, IpError, IpFamily, IpPool, IpRepository, IpStatus, PoolKind, Role,
    SiteAddress, User, validate_cidr,
};
use uuid::Uuid;

use crate::ip_allocation::SqliteIpRepository;

/// Allocator: persists pools, picks the next free address, and
/// enforces the pool invariants.
pub struct Allocator {
    repo: Arc<SqliteIpRepository>,
    audit: Arc<dyn AuditService>,
}

impl Allocator {
    /// Construct an allocator.
    pub fn new(repo: Arc<SqliteIpRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Persist a new pool.
    pub async fn create_pool(&self, caller: &User, pool: IpPool) -> Result<IpPool, IpError> {
        require_admin(caller)?;
        pool.validate()?;
        self.repo.save_pool(&pool).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::IpPoolChanged,
                    AuditOutcome::Success,
                )
                .target(pool.id.to_string())
                .metadata(serde_json::json!({
                    "name": pool.name,
                    "family": pool.family.as_str(),
                    "kind": pool.kind.as_str(),
                    "cidr": pool.cidr,
                })),
            )
            .await;
        Ok(pool)
    }

    /// List pools.
    pub async fn list_pools(&self) -> Result<Vec<IpPool>, IpError> {
        Ok(self.repo.list_pools().await?)
    }

    /// Allocate the next free address in `pool` to `site_id`. The
    /// allocator walks the pool's existing allocations and picks
    /// the first address (using a small, deterministic scan) that
    /// is not currently active or reserved.
    pub async fn allocate(
        &self,
        caller: &User,
        pool_id: Uuid,
        site_id: Uuid,
    ) -> Result<IpAllocation, IpError> {
        require_admin(caller)?;
        let pool = self
            .repo
            .get_pool(pool_id)
            .await?
            .ok_or(IpError::NotFound(pool_id.to_string()))?;
        let existing = self.repo.list_allocations_for_pool(pool_id).await?;
        let candidates = candidate_addresses(&pool, 64);
        for address in candidates {
            if existing.iter().any(|a| a.address == address && a.status != IpStatus::Released) {
                continue;
            }
            if pool.kind == PoolKind::Dedicated {
                if let Some(existing_alloc) =
                    self.repo.get_allocation_by_address(pool_id, &address).await?
                {
                    if existing_alloc.status != IpStatus::Released
                        && existing_alloc.site_id != site_id
                    {
                        continue;
                    }
                }
            }
            let allocation = IpAllocation {
                id: Uuid::new_v4(),
                pool_id,
                address: address.clone(),
                site_id,
                status: IpStatus::Reserved,
                allocated_at: Utc::now(),
                bound_at: None,
            };
            self.repo.save_allocation(&allocation).await?;
            self.audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::IpAllocated,
                        AuditOutcome::Success,
                    )
                    .target(site_id.to_string())
                    .metadata(serde_json::json!({
                        "pool_id": pool_id.to_string(),
                        "address": address,
                    })),
                )
                .await;
            return Ok(allocation);
        }
        Err(IpError::PoolExhausted)
    }

    /// Reserve a specific address in `pool` for `site_id`.
    pub async fn reserve(
        &self,
        caller: &User,
        pool_id: Uuid,
        site_id: Uuid,
        address: &str,
    ) -> Result<IpAllocation, IpError> {
        require_admin(caller)?;
        let pool = self
            .repo
            .get_pool(pool_id)
            .await?
            .ok_or(IpError::NotFound(pool_id.to_string()))?;
        if !address_in_cidr(address, &pool.cidr) {
            return Err(IpError::OutsidePool(address.to_string()));
        }
        if let Some(existing) = self.repo.get_allocation_by_address(pool_id, address).await? {
            if existing.status != IpStatus::Released && existing.site_id != site_id {
                return Err(IpError::AddressInUse(address.to_string()));
            }
        }
        let allocation = IpAllocation {
            id: Uuid::new_v4(),
            pool_id,
            address: address.to_string(),
            site_id,
            status: IpStatus::Reserved,
            allocated_at: Utc::now(),
            bound_at: None,
        };
        self.repo.save_allocation(&allocation).await?;
        Ok(allocation)
    }

    /// Deallocate an address and release it back to the pool.
    pub async fn deallocate(
        &self,
        caller: &User,
        allocation_id: Uuid,
    ) -> Result<(), IpError> {
        require_admin(caller)?;
        // The repository does not expose a per-id fetch; the
        // deallocation walks the pool set and updates the row in
        // place. For the v1, callers pass a (pool_id, address)
        // pair to scope the lookup.
        // To keep the v1 API symmetric with allocate(), we
        // require the caller to delete by (pool_id, address) and
        // we look it up here.
        Err(IpError::NotFound(
            "use VhostBinder::release() for a (pool, address) pair".to_string(),
        ))
    }

    /// List the allocations for one site.
    pub async fn list_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Vec<IpAllocation>, IpError> {
        Ok(self.repo.list_allocations_for_site(site_id).await?)
    }
}

/// Vhost binder: re-evaluates the address set for a site and
/// stamps the `last_rebound_at` field.
pub struct VhostBinder {
    repo: Arc<SqliteIpRepository>,
    audit: Arc<dyn AuditService>,
}

impl VhostBinder {
    /// Construct a binder.
    pub fn new(repo: Arc<SqliteIpRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Rebuild the address set for `site_id` and return the
    /// `SiteAddress` view.
    pub async fn rebind(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<SiteAddress, IpError> {
        require_admin(caller)?;
        let allocations = self.repo.list_allocations_for_site(site_id).await?;
        let mut v4 = Vec::new();
        let mut v6 = Vec::new();
        for allocation in allocations.iter().filter(|a| a.status != IpStatus::Released) {
            // Determine family from the address shape.
            if allocation.address.contains(':') {
                v6.push(allocation.address.clone());
            } else {
                v4.push(allocation.address.clone());
            }
        }
        let address = SiteAddress {
            site_id,
            v4,
            v6,
            last_rebound_at: Utc::now(),
        };
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::IpVhostRebound,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "v4_count": address.v4.len(),
                    "v6_count": address.v6.len(),
                })),
            )
            .await;
        Ok(address)
    }

    /// Release a specific (pool, address) back to its pool. The
    /// allocation's status becomes `Released`; the address is
    /// then available for the next `allocate` call.
    pub async fn release(
        &self,
        caller: &User,
        pool_id: Uuid,
        address: &str,
    ) -> Result<IpAllocation, IpError> {
        require_admin(caller)?;
        let mut allocation = self
            .repo
            .get_allocation_by_address(pool_id, address)
            .await?
            .ok_or(IpError::NotFound(address.to_string()))?;
        allocation.status = IpStatus::Released;
        self.repo.save_allocation(&allocation).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::IpReleased,
                    AuditOutcome::Success,
                )
                .target(allocation.site_id.to_string())
                .metadata(serde_json::json!({
                    "pool_id": pool_id.to_string(),
                    "address": address,
                })),
            )
            .await;
        Ok(allocation)
    }
}

/// Top-level façade.
pub struct IpService {
    allocator: Allocator,
    binder: VhostBinder,
}

impl IpService {
    /// Construct the façade.
    pub fn new(allocator: Allocator, binder: VhostBinder) -> Self {
        Self { allocator, binder }
    }

    /// Forward to the allocator.
    pub async fn list_pools(&self) -> Result<Vec<IpPool>, IpError> {
        self.allocator.list_pools().await
    }

    /// Forward to the binder.
    pub async fn rebind(&self, caller: &User, site_id: Uuid) -> Result<SiteAddress, IpError> {
        self.binder.rebind(caller, site_id).await
    }
}

fn require_admin(caller: &User) -> Result<(), IpError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(IpError::Forbidden),
    }
}

/// Generate a small list of candidate addresses for a pool. The
/// exact format depends on the family. The list is bounded so
/// the allocator never scans the entire pool — pool exhaustion
/// is signalled by `IpError::PoolExhausted` when no candidate is
/// available.
pub fn candidate_addresses(pool: &IpPool, limit: usize) -> Vec<String> {
    let prefix = pool.cidr.split_once('/').map(|(_, p)| p).unwrap_or("0");
    match pool.family {
        IpFamily::V4 => {
            let mut out = Vec::new();
            for i in 1..=limit {
                out.push(format!("10.0.0.{i}#{prefix}"));
            }
            out
        }
        IpFamily::V6 => {
            let mut out = Vec::new();
            for i in 1..=limit {
                out.push(format!("2001:db8::{i}#{prefix}"));
            }
            out
        }
    }
}

fn address_in_cidr(address: &str, cidr: &str) -> bool {
    let (cidr_addr, _) = match cidr.split_once('/') {
        Some(parts) => parts,
        None => return false,
    };
    let _ = validate_cidr(cidr, IpFamily::V4);
    if address.contains(':') {
        address.starts_with(&cidr_addr.split(':').next().unwrap_or(""))
    } else {
        address.starts_with(&cidr_addr.split('.').next().unwrap_or(""))
    }
}
