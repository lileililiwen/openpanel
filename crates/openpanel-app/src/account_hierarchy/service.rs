//! Account-hierarchy application service: parent-child relationships,
//! tree traversal, pool declaration, and pool claims.

use std::{collections::HashMap, sync::Arc};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AccountHierarchyError, AccountRelationship, HierarchyNode, HierarchyRepository, IdentityError,
    PoolAxis, PoolClaim, QuotaPool, Role, User, UserRepository, check_pool_claim,
    identity::repository::UserRepository as _,
};
use uuid::Uuid;

use crate::{account_hierarchy::repo::SqliteHierarchyRepository, identity::IdentityService};

const POOL_EXHAUSTED_REFUSAL: &str = "pool_exhausted";
const FORBIDDEN_REFUSAL: &str = "forbidden";

/// Child-account creation request.
pub struct CreateChildRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    pub role: Role,
    pub initial_plan_id: Option<openpanel_domain::HostingPlanId>,
}

impl std::fmt::Debug for CreateChildRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateChildRequest")
            .field("username", &self.username)
            .field("email", &self.email)
            .field("role", &self.role)
            .field("has_password", &"<redacted>")
            .field("initial_plan_id", &self.initial_plan_id)
            .finish()
    }
}

/// Account-hierarchy application service.
#[derive(Clone)]
pub struct HierarchyService {
    repo: Arc<dyn HierarchyRepository>,
    users: Arc<dyn UserRepository>,
    audit: Arc<dyn AuditService>,
}

impl HierarchyService {
    /// Build a service over the given repositories.
    pub fn new(
        repo: Arc<dyn HierarchyRepository>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self { repo, users, audit }
    }

    /// Build a service backed by the SQLite adapter.
    pub fn with_sqlite(
        pool: sqlx::Pool<sqlx::Sqlite>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self::new(Arc::new(SqliteHierarchyRepository::new(pool)), users, audit)
    }

    /// Attach a child to a parent. The caller MUST be the parent
    /// or an Owner. Returns `Cycle` if the proposed parent is
    /// itself a descendant of the child (or the child itself).
    pub async fn attach_child(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        actor: &str,
        actor_id: Uuid,
    ) -> Result<AccountRelationship, AccountHierarchyError> {
        if parent_id == child_id {
            return Err(AccountHierarchyError::Cycle);
        }
        let parent = self
            .users
            .find_by_id(parent_id)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?
            .ok_or(AccountHierarchyError::UserNotFound)?;
        let _ = parent;
        let child = self
            .users
            .find_by_id(child_id)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?
            .ok_or(AccountHierarchyError::UserNotFound)?;
        let _ = child;
        if self.repo.parent_of(child_id).await?.is_some() {
            return Err(AccountHierarchyError::AlreadyAttached);
        }
        if self.repo.path_exists(parent_id, child_id).await? {
            return Err(AccountHierarchyError::Cycle);
        }
        let now = Utc::now();
        let rel = AccountRelationship::new(parent_id, child_id, actor_id, now);
        self.repo.insert_relationship(&rel).await?;
        self.users
            .update_parent_account_id(child_id, Some(parent_id))
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyAttached,
                    AuditOutcome::Success,
                )
                .target(child_id.to_string())
                .metadata(serde_json::json!({"parent_id": parent_id.to_string()})),
            )
            .await
            .ok();
        Ok(rel)
    }

    /// Detach a child from a parent. Returns `UserNotFound` if
    /// the child has no active parent.
    pub async fn detach_child(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        actor: &str,
    ) -> Result<(), AccountHierarchyError> {
        let rel = self
            .repo
            .parent_of(child_id)
            .await?
            .ok_or(AccountHierarchyError::UserNotFound)?;
        if rel.parent_id() != parent_id {
            return Err(AccountHierarchyError::UserNotFound);
        }
        self.repo.detach(parent_id, child_id).await?;
        self.users
            .update_parent_account_id(child_id, None)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyDetached,
                    AuditOutcome::Success,
                )
                .target(child_id.to_string())
                .metadata(serde_json::json!({"parent_id": parent_id.to_string()})),
            )
            .await
            .ok();
        Ok(())
    }

    /// Return the children of a parent.
    pub async fn children(&self, parent_id: Uuid) -> Result<Vec<User>, AccountHierarchyError> {
        let rels = self.repo.children_of(parent_id).await?;
        let mut out = Vec::with_capacity(rels.len());
        for rel in rels {
            if let Some(user) = self
                .users
                .find_by_id(rel.child_id())
                .await
                .map_err(|e| AccountHierarchyError::Persistence(e.0))?
            {
                out.push(user);
            }
        }
        Ok(out)
    }

    /// Build the recursive tree under a root.
    pub async fn tree(&self, root_id: Uuid) -> Result<HierarchyNode, AccountHierarchyError> {
        let root = self
            .users
            .find_by_id(root_id)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?
            .ok_or(AccountHierarchyError::UserNotFound)?;
        let mut root_node = HierarchyNode::new(root.id(), root.parent_account_id(), 0);
        // Build a parent -> children map, then walk recursively.
        let mut by_parent: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        for user in self
            .users
            .list()
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?
        {
            if let Some(parent) = user.parent_account_id() {
                by_parent.entry(parent).or_default().push(user.id());
            }
        }
        self.append_children(&mut root_node, &by_parent, 0);
        Ok(root_node)
    }

    fn append_children(
        &self,
        node: &mut HierarchyNode,
        by_parent: &HashMap<Uuid, Vec<Uuid>>,
        depth: u32,
    ) {
        if let Some(kids) = by_parent.get(&node.user_id()) {
            for kid in kids {
                let parent_id = by_parent
                    .get(kid)
                    .and_then(|p| p.first().copied())
                    .unwrap_or(node.user_id());
                let mut child = HierarchyNode::new(*kid, Some(parent_id), depth + 1);
                self.append_children(&mut child, by_parent, depth + 1);
                node.push_child(child);
            }
        }
    }

    /// Declare or update a pool on a parent.
    pub async fn set_pool(
        &self,
        parent_id: Uuid,
        axis: PoolAxis,
        total_bytes: u64,
        actor: &str,
    ) -> Result<QuotaPool, AccountHierarchyError> {
        let now = Utc::now();
        let pool = QuotaPool::new(parent_id, axis, total_bytes, now)?;
        self.repo.upsert_pool(&pool).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyPoolSet,
                    AuditOutcome::Success,
                )
                .target(parent_id.to_string())
                .metadata(serde_json::json!({
                    "axis": pool_axis_label(axis),
                    "total_bytes": total_bytes,
                })),
            )
            .await
            .ok();
        Ok(pool)
    }

    /// Claim a share from a parent's pool. Refuses when the
    /// pool is exhausted.
    pub async fn claim_pool(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        axis: PoolAxis,
        share_bytes: u64,
        actor: &str,
    ) -> Result<PoolClaim, AccountHierarchyError> {
        let pools = self.repo.pools_of(parent_id).await?;
        let pool = pools
            .iter()
            .find(|p| p.axis() == axis)
            .ok_or(AccountHierarchyError::PoolNotFound)?;
        let used = self.repo.pool_usage(parent_id, axis).await?;
        check_pool_claim(pool.total_bytes(), used, share_bytes)?;
        let now = Utc::now();
        let claim = PoolClaim::new(parent_id, child_id, axis, share_bytes, now)?;
        self.repo.insert_claim(&claim).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyPoolClaimed,
                    AuditOutcome::Success,
                )
                .target(child_id.to_string())
                .metadata(serde_json::json!({
                    "parent_id": parent_id.to_string(),
                    "axis": pool_axis_label(axis),
                    "share_bytes": share_bytes,
                })),
            )
            .await
            .ok();
        Ok(claim)
    }

    /// Release a claim.
    pub async fn release_claim(
        &self,
        parent_id: Uuid,
        child_id: Uuid,
        axis: PoolAxis,
        actor: &str,
    ) -> Result<(), AccountHierarchyError> {
        self.repo.remove_claim(parent_id, child_id, axis).await?;
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyPoolReleased,
                    AuditOutcome::Success,
                )
                .target(child_id.to_string())
                .metadata(serde_json::json!({
                    "parent_id": parent_id.to_string(),
                    "axis": pool_axis_label(axis),
                })),
            )
            .await
            .ok();
        Ok(())
    }

    /// Return the usage breakdown for a parent.
    pub async fn pool_usage(
        &self,
        parent_id: Uuid,
    ) -> Result<Vec<PoolUsageRow>, AccountHierarchyError> {
        let pools = self.repo.pools_of(parent_id).await?;
        let mut out = Vec::with_capacity(pools.len());
        for pool in pools {
            let used = self.repo.pool_usage(parent_id, pool.axis()).await?;
            out.push(PoolUsageRow {
                axis: pool.axis(),
                total_bytes: pool.total_bytes(),
                used_bytes: used,
            });
        }
        Ok(out)
    }

    /// Create a child account owned by the parent.
    pub async fn create_child(
        &self,
        parent_id: Uuid,
        request: CreateChildRequest,
        actor: &str,
        actor_id: Uuid,
    ) -> Result<User, AccountHierarchyError> {
        let parent = self
            .users
            .find_by_id(parent_id)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?
            .ok_or(AccountHierarchyError::UserNotFound)?;
        if !can_create_child(parent.role(), request.role) {
            return Err(AccountHierarchyError::Forbidden);
        }
        let password = openpanel_domain::Password::hash(&request.password)
            .map_err(|_| AccountHierarchyError::Persistence("password too short".to_string()))?;
        let username = openpanel_domain::Username::new(request.username.clone())
            .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        let email = openpanel_domain::Email::new(request.email.clone())
            .map_err(|e| AccountHierarchyError::Persistence(e.to_string()))?;
        let user = openpanel_domain::User::new_with_parents(
            Uuid::new_v4(),
            username,
            email,
            password,
            request.role,
            Some(parent_id),
            request.initial_plan_id,
        )
        .map_err(map_identity_err)?;
        self.users
            .insert(&user)
            .await
            .map_err(|e| AccountHierarchyError::Persistence(e.0))?;
        let now = Utc::now();
        let rel = AccountRelationship::new(parent_id, user.id(), actor_id, now);
        self.repo.insert_relationship(&rel).await?;
        if let Some(plan_id) = request.initial_plan_id {
            self.users
                .update_hosting_plan_id(user.id(), Some(plan_id))
                .await
                .map_err(|e| AccountHierarchyError::Persistence(e.0))?;
        }
        self.audit
            .record(
                AuditEvent::new(
                    actor,
                    AuditAction::AccountHierarchyChildCreated,
                    AuditOutcome::Success,
                )
                .target(user.id().to_string())
                .metadata(serde_json::json!({"parent_id": parent_id.to_string()})),
            )
            .await
            .ok();
        Ok(user)
    }
}

/// Per-axis usage breakdown for a parent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolUsageRow {
    pub axis: PoolAxis,
    pub total_bytes: u64,
    pub used_bytes: u64,
}

fn pool_axis_label(axis: PoolAxis) -> &'static str {
    match axis {
        PoolAxis::DiskBytes => "disk_bytes",
        PoolAxis::BandwidthBytesPerMonth => "bandwidth_bytes_per_month",
        PoolAxis::MaxChildAccounts => "max_child_accounts",
    }
}

fn can_create_child(caller: Role, child: Role) -> bool {
    matches!(
        (caller, child),
        (Role::Owner, Role::Admin) | (Role::Owner, Role::User) | (Role::Admin, Role::User)
    )
}

fn map_identity_err(err: IdentityError) -> AccountHierarchyError {
    match err {
        IdentityError::ParentAccountCycle => AccountHierarchyError::Cycle,
        IdentityError::UserNotFound => AccountHierarchyError::UserNotFound,
        IdentityError::Forbidden => AccountHierarchyError::Forbidden,
        e => AccountHierarchyError::Persistence(e.to_string()),
    }
}

// Avoid dead_code noise for the unused constants.
#[allow(dead_code)]
const _REFUSAL_KIND: &str = FORBIDDEN_REFUSAL;
const _POOL_EXHAUSTED: &str = POOL_EXHAUSTED_REFUSAL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_matrix_is_correct() {
        assert!(can_create_child(Role::Owner, Role::Admin));
        assert!(can_create_child(Role::Owner, Role::User));
        assert!(can_create_child(Role::Admin, Role::User));
        assert!(!can_create_child(Role::Admin, Role::Admin));
        assert!(!can_create_child(Role::User, Role::User));
        assert!(!can_create_child(Role::Owner, Role::Owner));
    }
}
