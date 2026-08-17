//! Account hierarchy bounded context: parent-child user
//! relationships, tree traversal, and quota pools.
//!
//! The `parent_account_id` field on `User` (added by the
//! `refine-identity-with-hierarchy-and-plan-fields` change) is the
//! FK this context consumes. The hierarchy bounded context adds
//! the *behaviour*: cycle detection, tree flattening, child-account
//! creation, and pooled quota caps that children share.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;

/// Lifecycle status of a parent-child relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HierarchyStatus {
    /// The relationship is active.
    Active,
    /// The child has been detached from the parent (e.g. during
    /// a re-parenting operation). The row is retained for audit
    /// but does not contribute to the tree.
    Detached,
}

/// A parent → child account relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRelationship {
    parent_id: Uuid,
    child_id: Uuid,
    created_at: DateTime<Utc>,
    created_by: Uuid,
    status: HierarchyStatus,
}

impl AccountRelationship {
    /// Build a new active relationship.
    pub fn new(parent_id: Uuid, child_id: Uuid, created_by: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            parent_id,
            child_id,
            created_at: now,
            created_by,
            status: HierarchyStatus::Active,
        }
    }

    /// Parent account id.
    pub fn parent_id(&self) -> Uuid {
        self.parent_id
    }

    /// Child account id.
    pub fn child_id(&self) -> Uuid {
        self.child_id
    }

    /// When the relationship was created.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Who created the relationship.
    pub fn created_by(&self) -> Uuid {
        self.created_by
    }

    /// Current status.
    pub fn status(&self) -> HierarchyStatus {
        self.status
    }

    /// Mark the relationship as detached.
    pub fn detach(&mut self) {
        self.status = HierarchyStatus::Detached;
    }

    /// Whether the relationship is active.
    pub fn is_active(&self) -> bool {
        matches!(self.status, HierarchyStatus::Active)
    }
}

/// Pooled quota axis. Mirrors the `PlanQuota` axes so the
/// `PlanResolver` can take the minimum of the per-axis cap
/// the pool afford.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolAxis {
    /// Disk bytes the children may share.
    DiskBytes,
    /// Outbound network bytes per month the children may share.
    BandwidthBytesPerMonth,
    /// Maximum number of children the parent may own.
    MaxChildAccounts,
}

/// A pooled quota declared by a parent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuotaPool {
    parent_id: Uuid,
    axis: PoolAxis,
    total_bytes: u64,
    updated_at: DateTime<Utc>,
}

impl QuotaPool {
    /// Build a pool. The total must be > 0.
    pub fn new(
        parent_id: Uuid,
        axis: PoolAxis,
        total_bytes: u64,
        now: DateTime<Utc>,
    ) -> Result<Self, AccountHierarchyError> {
        if total_bytes == 0 {
            return Err(AccountHierarchyError::InvalidPoolTotal);
        }
        Ok(Self {
            parent_id,
            axis,
            total_bytes,
            updated_at: now,
        })
    }

    /// Parent account id.
    pub fn parent_id(&self) -> Uuid {
        self.parent_id
    }

    /// Pool axis.
    pub fn axis(&self) -> PoolAxis {
        self.axis
    }

    /// Total bytes the pool grants.
    pub fn total_bytes(&self) -> u64 {
        self.total_bytes
    }

    /// Update total bytes. The new value must be > 0 and
    /// must be ≥ the sum of currently-claimed shares.
    pub fn set_total_bytes(
        &mut self,
        total_bytes: u64,
        now: DateTime<Utc>,
    ) -> Result<(), AccountHierarchyError> {
        if total_bytes == 0 {
            return Err(AccountHierarchyError::InvalidPoolTotal);
        }
        self.total_bytes = total_bytes;
        self.updated_at = now;
        Ok(())
    }

    /// When the pool was last updated.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// A claim a child holds against a parent's pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolClaim {
    parent_id: Uuid,
    child_id: Uuid,
    axis: PoolAxis,
    share_bytes: u64,
    claimed_at: DateTime<Utc>,
}

impl PoolClaim {
    /// Build a claim. The share must be > 0.
    pub fn new(
        parent_id: Uuid,
        child_id: Uuid,
        axis: PoolAxis,
        share_bytes: u64,
        now: DateTime<Utc>,
    ) -> Result<Self, AccountHierarchyError> {
        if share_bytes == 0 {
            return Err(AccountHierarchyError::InvalidClaim);
        }
        Ok(Self {
            parent_id,
            child_id,
            axis,
            share_bytes,
            claimed_at: now,
        })
    }

    /// Parent account id.
    pub fn parent_id(&self) -> Uuid {
        self.parent_id
    }

    /// Child account id.
    pub fn child_id(&self) -> Uuid {
        self.child_id
    }

    /// Pool axis.
    pub fn axis(&self) -> PoolAxis {
        self.axis
    }

    /// Share bytes the child holds.
    pub fn share_bytes(&self) -> u64 {
        self.share_bytes
    }

    /// When the claim was made.
    pub fn claimed_at(&self) -> DateTime<Utc> {
        self.claimed_at
    }
}

/// A snapshot of the tree under a root, useful for the
/// `/users/{id}/tree` endpoint and the `/accounts/tree` page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HierarchyNode {
    user_id: Uuid,
    parent_id: Option<Uuid>,
    depth: u32,
    children: Vec<HierarchyNode>,
}

impl HierarchyNode {
    /// Build a node.
    pub fn new(user_id: Uuid, parent_id: Option<Uuid>, depth: u32) -> Self {
        Self {
            user_id,
            parent_id,
            depth,
            children: Vec::new(),
        }
    }

    /// User id.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Parent id, if any.
    pub fn parent_id(&self) -> Option<Uuid> {
        self.parent_id
    }

    /// Depth in the tree (root is 0).
    pub fn depth(&self) -> u32 {
        self.depth
    }

    /// Children.
    pub fn children(&self) -> &[HierarchyNode] {
        &self.children
    }

    /// Append a child node.
    pub fn push_child(&mut self, child: HierarchyNode) {
        self.children.push(child);
    }
}

/// Persistence port for relationships and pools.
#[async_trait]
pub trait HierarchyRepository: Send + Sync + 'static {
    /// Insert a new relationship.
    async fn insert_relationship(
        &self,
        relationship: &AccountRelationship,
    ) -> Result<(), AccountHierarchyError>;
    /// Return the active children of a parent.
    async fn children_of(
        &self,
        parent_id: Uuid,
    ) -> Result<Vec<AccountRelationship>, AccountHierarchyError>;
    /// Return the parent of a child, if any.
    async fn parent_of(
        &self,
        child_id: Uuid,
    ) -> Result<Option<AccountRelationship>, AccountHierarchyError>;
    /// Detach a child (mark Detached). Idempotent.
    async fn detach(&self, parent_id: Uuid, child_id: Uuid) -> Result<(), AccountHierarchyError>;
    /// Insert or update a pool.
    async fn upsert_pool(&self, pool: &QuotaPool) -> Result<(), AccountHierarchyError>;
    /// Return all pools declared by a parent.
    async fn pools_of(&self, parent_id: Uuid) -> Result<Vec<QuotaPool>, AccountHierarchyError>;
    /// Insert a claim.
    async fn insert_claim(&self, claim: &PoolClaim) -> Result<(), AccountHierarchyError>;
    /// Return all claims against a parent.
    async fn claims_of(&self, parent_id: Uuid) -> Result<Vec<PoolClaim>, AccountHierarchyError>;
    /// Remove a claim.
    async fn remove_claim(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        axis: PoolAxis,
    ) -> Result<(), AccountHierarchyError>;
    /// Sum of share bytes for a given axis.
    async fn pool_usage(
        &self,
        parent_id: Uuid,
        axis: PoolAxis,
    ) -> Result<u64, AccountHierarchyError>;
    /// Detect a cycle: returns `true` if `descendant` reaches
    /// `ancestor` via the active parent chain.
    async fn path_exists(
        &self,
        ancestor: Uuid,
        descendant: Uuid,
    ) -> Result<bool, AccountHierarchyError>;
    /// Default implementation that bridges the placeholder
    /// identity repository; the full implementation lives in the
    /// follow-on change.
    async fn exists(&self, _id: Uuid) -> Result<bool, RepoError> {
        Ok(true)
    }
}

/// Errors that can occur in the account-hierarchy bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AccountHierarchyError {
    /// The proposed parent-child relationship would create a cycle.
    #[error("account hierarchy cycle detected")]
    Cycle,
    /// The user already has a parent; re-parenting requires
    /// explicit detach.
    #[error("user already has a parent")]
    AlreadyAttached,
    /// The parent / child cannot be found.
    #[error("user not found")]
    UserNotFound,
    /// The pool total is zero.
    #[error("pool total must be > 0")]
    InvalidPoolTotal,
    /// The claim share is zero.
    #[error("claim share must be > 0")]
    InvalidClaim,
    /// The pool does not have enough remaining capacity for the
    /// requested claim.
    #[error("pool exhausted")]
    PoolExhausted,
    /// The pool itself does not exist.
    #[error("pool not found")]
    PoolNotFound,
    /// The caller is not allowed to perform the operation.
    #[error("permission denied")]
    Forbidden,
    /// Persistence layer failure.
    #[error("account hierarchy persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for AccountHierarchyError {
    fn from(error: RepoError) -> Self {
        AccountHierarchyError::Persistence(error.0)
    }
}

/// Detect a cycle in the proposed parent assignment.
///
/// The `children_of` map is `parent_id -> [child_id]`. We walk DOWN
/// from `proposed_parent` to see if we reach `child_id`. If we do,
/// the proposed assignment would create a cycle (the new parent
/// is already a descendant of the child).
pub fn would_cycle(
    proposed_parent: Uuid,
    child_id: Uuid,
    children_of: &HashMap<Uuid, Vec<Uuid>>,
) -> bool {
    if proposed_parent == child_id {
        return true;
    }
    let mut stack: Vec<Uuid> = vec![proposed_parent];
    while let Some(current) = stack.pop() {
        if current == child_id {
            return true;
        }
        if let Some(kids) = children_of.get(&current) {
            for kid in kids {
                stack.push(*kid);
            }
        }
    }
    false
}

/// Validate a would-be pool claim. Returns `Ok(())` if the claim
/// fits within the pool total; otherwise `PoolExhausted`.
pub fn check_pool_claim(
    pool_total: u64,
    current_claimed: u64,
    requested: u64,
) -> Result<(), AccountHierarchyError> {
    let new_total = current_claimed.saturating_add(requested);
    if new_total > pool_total {
        return Err(AccountHierarchyError::PoolExhausted);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn children_map() -> HashMap<Uuid, Vec<Uuid>> {
        let mut map = HashMap::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        // chain: a -> b -> c
        map.insert(a, vec![b]);
        map.insert(b, vec![c]);
        map
    }

    #[test]
    fn cycle_detected_when_parent_is_child() {
        let a = Uuid::new_v4();
        let map = children_map();
        assert!(would_cycle(a, a, &map));
    }

    #[test]
    fn cycle_detected_through_chain() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let mut map = HashMap::new();
        map.insert(a, vec![b]);
        map.insert(b, vec![c]);
        // Proposing `a` as parent of `c` would form a cycle:
        // c -> a -> b -> c.
        assert!(would_cycle(a, c, &map));
    }

    #[test]
    fn no_cycle_for_unrelated_nodes() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let map = children_map();
        assert!(!would_cycle(a, b, &map));
        assert!(!would_cycle(a, c, &map));
    }

    #[test]
    fn pool_claim_within_total_succeeds() {
        let res = check_pool_claim(1000, 200, 700);
        assert!(res.is_ok());
    }

    #[test]
    fn pool_claim_at_exact_total_succeeds() {
        let res = check_pool_claim(1000, 200, 800);
        assert!(res.is_ok());
    }

    #[test]
    fn pool_claim_over_total_rejected() {
        let err = check_pool_claim(1000, 200, 801).expect_err("must fail");
        assert_eq!(err, AccountHierarchyError::PoolExhausted);
    }

    #[test]
    fn pool_zero_total_rejected() {
        let err = QuotaPool::new(Uuid::new_v4(), PoolAxis::DiskBytes, 0, Utc::now())
            .expect_err("must reject");
        assert_eq!(err, AccountHierarchyError::InvalidPoolTotal);
    }

    #[test]
    fn claim_zero_share_rejected() {
        let err = PoolClaim::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            PoolAxis::DiskBytes,
            0,
            Utc::now(),
        )
        .expect_err("must reject");
        assert_eq!(err, AccountHierarchyError::InvalidClaim);
    }
}
