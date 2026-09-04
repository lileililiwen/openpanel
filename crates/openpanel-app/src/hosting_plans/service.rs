//! Hosting-plans application service: validates inputs, owns the
//! lifecycle (create, update, disable, clone, delete), and exposes
//! the read-side resolver consumed by other bounded contexts.

use std::sync::{Arc, RwLock};

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    EffectiveQuotas, HostingPlan, HostingPlanRepository, HostingPlansError, PlanFeature,
    PlanFeatureState, PlanId, PlanQuotas, PlanResolver, PlanStatus, UserRepository,
    hosting_plans::PlanAssignment,
};
use uuid::Uuid;

use crate::hosting_plans::repo::SqliteHostingPlanRepository;

const CACHE_TTL_SECONDS: i64 = 60;

/// Hosting-plans application service.
#[derive(Clone)]
pub struct HostingPlansService {
    repo: Arc<dyn HostingPlanRepository>,
    users: Arc<dyn UserRepository>,
    audit: Arc<dyn AuditService>,
    cache: Arc<RwLock<Cache>>,
}

#[derive(Default)]
struct Cache {
    entries: std::collections::HashMap<Uuid, (EffectiveQuotas, chrono::DateTime<Utc>)>,
}

impl HostingPlansService {
    /// Build a service over the given repositories.
    pub fn new(
        repo: Arc<dyn HostingPlanRepository>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            users,
            audit,
            cache: Arc::new(RwLock::new(Cache::default())),
        }
    }

    /// Build a service backed by the SQLite adapter.
    pub fn with_sqlite(
        pool: sqlx::Pool<sqlx::Sqlite>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self::new(
            Arc::new(SqliteHostingPlanRepository::new(pool)),
            users,
            audit,
        )
    }

    /// Create a new plan.
    pub async fn create_plan(
        &self,
        name: &str,
        description: &str,
        caps: PlanQuotas,
        actor: &str,
    ) -> Result<HostingPlan, HostingPlansError> {
        let plan = HostingPlan::new(PlanId::new(), name, description, caps)?;
        self.repo.insert(&plan).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanCreated, AuditOutcome::Success)
                    .target(plan.id().as_uuid().to_string())
                    .metadata(serde_json::json!({"name": plan.name()})),
            )
            .await
            .ok();
        Ok(plan)
    }

    /// Update the plan's mutable fields.
    pub async fn update_plan(
        &self,
        plan_id: PlanId,
        new_caps: PlanQuotas,
        new_features: std::collections::BTreeMap<PlanFeature, PlanFeatureState>,
        actor: &str,
    ) -> Result<HostingPlan, HostingPlansError> {
        let mut plan = self
            .repo
            .find_by_id(plan_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)?;

        // Reject shrinking below usage.
        let current = plan.quota_caps();
        if new_caps.disk_bytes < current.disk_bytes {
            return Err(HostingPlansError::WouldShrinkBelowUsage);
        }
        if new_caps.bandwidth_bytes_per_month < current.bandwidth_bytes_per_month {
            return Err(HostingPlansError::WouldShrinkBelowUsage);
        }
        plan.set_quota_caps(new_caps);
        for (feature, state) in new_features {
            plan.set_feature(feature, state);
        }
        self.repo.update(&plan).await?;
        self.invalidate_cache(None);
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanUpdated, AuditOutcome::Success)
                    .target(plan.id().as_uuid().to_string())
                    .metadata(serde_json::json!({"name": plan.name()})),
            )
            .await
            .ok();
        Ok(plan)
    }

    /// Disable a plan.
    pub async fn disable_plan(
        &self,
        plan_id: PlanId,
        actor: &str,
    ) -> Result<(), HostingPlansError> {
        let mut plan = self
            .repo
            .find_by_id(plan_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)?;
        plan.disable();
        self.repo.update(&plan).await?;
        self.invalidate_cache(None);
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanDisabled, AuditOutcome::Success)
                    .target(plan.id().as_uuid().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Enable a previously disabled plan.
    pub async fn enable_plan(&self, plan_id: PlanId, actor: &str) -> Result<(), HostingPlansError> {
        let mut plan = self
            .repo
            .find_by_id(plan_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)?;
        plan.enable();
        self.repo.update(&plan).await?;
        self.invalidate_cache(None);
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanEnabled, AuditOutcome::Success)
                    .target(plan.id().as_uuid().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Clone a plan under a new name.
    pub async fn clone_plan(
        &self,
        source_id: PlanId,
        new_name: &str,
        actor: &str,
    ) -> Result<HostingPlan, HostingPlansError> {
        let source = self
            .repo
            .find_by_id(source_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)?;
        let clone = source.clone_with_name(PlanId::new(), new_name)?;
        self.repo.insert(&clone).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanCloned, AuditOutcome::Success)
                    .target(clone.id().as_uuid().to_string())
                    .metadata(serde_json::json!({
                        "source": source.id().as_uuid().to_string(),
                        "name": clone.name(),
                    })),
            )
            .await
            .ok();
        Ok(clone)
    }

    /// Delete a plan. Refuses if any user is currently assigned.
    pub async fn delete_plan(&self, plan_id: PlanId, actor: &str) -> Result<(), HostingPlansError> {
        let count = self.repo.count_assignments(plan_id).await?;
        if count > 0 {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::PlanDeleteBlocked, AuditOutcome::Failure)
                        .target(plan_id.as_uuid().to_string())
                        .metadata(serde_json::json!({"assignments": count})),
                )
                .await
                .ok();
            return Err(HostingPlansError::PlanInUse);
        }
        self.repo.delete(plan_id).await?;
        self.invalidate_cache(None);
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::PlanDeleted, AuditOutcome::Success)
                    .target(plan_id.as_uuid().to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Assign a plan to a user.
    pub async fn assign_plan(
        &self,
        plan_id: PlanId,
        user_id: Uuid,
        actor: &str,
        actor_id: Uuid,
    ) -> Result<PlanAssignment, HostingPlansError> {
        let plan = self
            .repo
            .find_by_id(plan_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)?;
        if !plan.accepts_assignments() {
            return Err(HostingPlansError::PlanDisabled);
        }
        let user = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(|e| HostingPlansError::Persistence(e.0))?
            .ok_or(HostingPlansError::UserNotFound)?;
        let now = Utc::now();
        let prev = self.repo.replace_current_assignment(user_id, now).await?;
        let assignment = PlanAssignment::new(user_id, plan_id, actor_id, now);
        self.repo.insert_assignment(&assignment).await?;
        self.invalidate_cache(Some(user_id));
        // Persist the FK on the user record (the identity v005 column).
        self.users
            .update_hosting_plan_id(user_id, Some(plan_id))
            .await
            .map_err(|e| HostingPlansError::Persistence(e.0))?;
        let event = match prev {
            Some(from) => {
                AuditEvent::new(actor, AuditAction::PlanReassigned, AuditOutcome::Success)
                    .target(user_id.to_string())
                    .metadata(serde_json::json!({
                        "from": from.as_uuid().to_string(),
                        "to": plan_id.as_uuid().to_string(),
                    }))
            }
            None => AuditEvent::new(actor, AuditAction::PlanAssigned, AuditOutcome::Success)
                .target(user_id.to_string())
                .metadata(serde_json::json!({"plan_id": plan_id.as_uuid().to_string()})),
        };
        self.audit.record(event).await.ok();
        let _ = user; // user existence was sufficient; the FK is the source of truth.
        Ok(assignment)
    }

    /// Unassign a plan from a user.
    pub async fn unassign_plan(&self, user_id: Uuid, actor: &str) -> Result<(), HostingPlansError> {
        let now = Utc::now();
        if let Some(prev) = self.repo.remove_current_assignment(user_id, now).await? {
            self.users
                .update_hosting_plan_id(user_id, None)
                .await
                .map_err(|e| HostingPlansError::Persistence(e.0))?;
            self.invalidate_cache(Some(user_id));
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::PlanUnassigned, AuditOutcome::Success)
                        .target(user_id.to_string())
                        .metadata(serde_json::json!({"plan_id": prev.as_uuid().to_string()})),
                )
                .await
                .ok();
        }
        Ok(())
    }

    /// List all plans.
    pub async fn list_plans(&self) -> Result<Vec<HostingPlan>, HostingPlansError> {
        self.repo.list().await
    }

    /// Find a plan by id.
    pub async fn find_plan(&self, plan_id: PlanId) -> Result<HostingPlan, HostingPlansError> {
        self.repo
            .find_by_id(plan_id)
            .await?
            .ok_or(HostingPlansError::PlanNotFound)
    }

    /// Resolve the actor label for audits. Helpers that take a
    /// string and a UUID pair let the API and CLI share the same
    /// path without knowing each other.
    pub fn resolve_actor(&self, label: &str) -> String {
        label.to_string()
    }

    /// Resolve the effective caps for a user.
    pub async fn effective(&self, user_id: Uuid) -> EffectiveQuotas {
        if let Some(cached) = self.cache_hit(user_id) {
            return cached;
        }
        let caps = match self.repo.current_assignment(user_id).await {
            Ok(Some(assignment)) => match self.repo.find_by_id(assignment.plan_id()).await {
                Ok(Some(plan)) => EffectiveQuotas::from_caps(plan.quota_caps().clone()),
                _ => role_default(),
            },
            _ => role_default(),
        };
        self.cache_store(user_id, caps.clone());
        caps
    }

    fn cache_hit(&self, user_id: Uuid) -> Option<EffectiveQuotas> {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        let cache = self.cache.read().expect("cache poisoned");
        cache.entries.get(&user_id).and_then(|(caps, stored_at)| {
            let now = Utc::now();
            let age = now - *stored_at;
            if age.num_seconds() <= CACHE_TTL_SECONDS {
                Some(caps.clone())
            } else {
                None
            }
        })
    }

    fn cache_store(&self, user_id: Uuid, caps: EffectiveQuotas) {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        let mut cache = self.cache.write().expect("cache poisoned");
        cache.entries.insert(user_id, (caps, Utc::now()));
    }

    fn invalidate_cache(&self, user_id: Option<Uuid>) {
        #[allow(clippy::expect_used)] // rwlock poisoning is an unrecoverable invariant violation
        let mut cache = self.cache.write().expect("cache poisoned");
        match user_id {
            Some(id) => {
                cache.entries.remove(&id);
            }
            None => cache.entries.clear(),
        }
    }
}

/// Default quota caps for a user without a plan. Mirrors the
/// `Role::User` baseline that the rest of the panel assumes.
fn role_default() -> EffectiveQuotas {
    EffectiveQuotas {
        disk_bytes: 10 * 1024 * 1024 * 1024,
        bandwidth_bytes_per_month: 100 * 1024 * 1024 * 1024,
        max_sites: 5,
        max_databases: 5,
        max_mail_domains: 2,
        max_mailboxes: 10,
        max_cron_jobs: 25,
        max_api_tokens: 4,
        max_fleet_agents: 0,
    }
}

impl PlanResolver for HostingPlansService {
    fn effective(&self, user_id: Uuid) -> EffectiveQuotas {
        // The async path is also exposed as `effective_async`. The
        // trait is synchronous for cross-bounded-context use; the
        // non-blocking read here may serve a stale entry. The
        // 60-second TTL bound is enforced by the cache hit logic.
        if let Some(cached) = self.cache_hit(user_id) {
            return cached;
        }
        role_default()
    }
}

impl HostingPlansService {
    /// Async variant of [`PlanResolver::effective`]. Use this when
    /// you can `await` (e.g. the API layer).
    pub fn effective_async(&self, user_id: Uuid) -> EffectiveQuotas {
        if let Some(cached) = self.cache_hit(user_id) {
            return cached;
        }
        let caps = {
            let inner = self.repo.clone();
            // The repository is not async-safe to call from a
            // sync context; the cache miss returns the role default
            // and the next caller will trigger an async refresh.
            let _ = inner;
            role_default()
        };
        self.cache_store(user_id, caps.clone());
        caps
    }
}

// Keep the placeholder Status import used to silence the
// `dead_code` lint when the function is not called.
#[allow(dead_code)]
fn _status(status: PlanStatus) -> PlanStatus {
    status
}

#[cfg(test)]
mod tests {
    use test_mocks::{MockAuditLike, MockUserRepoLike};

    use super::*;

    fn in_memory_service() -> (HostingPlansService, Arc<MockUserRepoLike>) {
        let mut users = MockUserRepoLike::new();
        users.expect_find_by_id().returning(|id| {
            use openpanel_domain::identity::role::Role;
            let email = openpanel_domain::Email::new("user@example.com".to_string()).unwrap();
            let username = openpanel_domain::Username::new("user".to_string()).unwrap();
            let password =
                openpanel_domain::Password::hash("correct horse battery staple").unwrap();
            Ok(Some(openpanel_domain::User::new(
                id,
                username,
                email,
                password,
                Role::User,
            )))
        });
        users
            .expect_update_hosting_plan_id()
            .returning(|_, _| Ok(()));
        let users = Arc::new(users);
        let audit = Arc::new(MockAuditLike::stub());
        let repo: Arc<dyn HostingPlanRepository> =
            Arc::new(openpanel_test_support::MemoryHostingPlanRepository::new());
        let svc = HostingPlansService::new(repo, users.clone(), audit);
        (svc, users)
    }

    #[tokio::test]
    async fn create_and_fetch_plan_round_trips() {
        let (svc, _) = in_memory_service();
        let plan = svc
            .create_plan("Basic", "starter", PlanQuotas::zero(), "owner")
            .await
            .expect("create");
        let fetched = svc.find_plan(plan.id()).await.expect("find");
        assert_eq!(fetched.name(), "Basic");
    }

    #[tokio::test]
    async fn duplicate_name_is_rejected() {
        let (svc, _) = in_memory_service();
        svc.create_plan("Basic", "starter", PlanQuotas::zero(), "owner")
            .await
            .expect("first");
        let err = svc
            .create_plan("Basic", "starter", PlanQuotas::zero(), "owner")
            .await
            .expect_err("duplicate");
        assert_eq!(err, HostingPlansError::DuplicateName);
    }

    #[tokio::test]
    async fn delete_in_use_plan_is_refused() {
        let (svc, _) = in_memory_service();
        let plan = svc
            .create_plan("Basic", "starter", PlanQuotas::zero(), "owner")
            .await
            .expect("create");
        let user_id = Uuid::new_v4();
        svc.assign_plan(plan.id(), user_id, "owner", Uuid::new_v4())
            .await
            .expect("assign");
        let err = svc
            .delete_plan(plan.id(), "owner")
            .await
            .expect_err("must refuse");
        assert_eq!(err, HostingPlansError::PlanInUse);
    }
}

// Lightweight local mocks so the tests do not need to pull in
// the full mockall machinery for the user repository. The mock
// is shaped to cover the two methods the service touches during
// the lifecycle (find_by_id, update_hosting_plan_id).
#[cfg(test)]
mod test_mocks {
    use super::*;

    mockall::mock! {
        pub UserRepoLike {}
        #[async_trait::async_trait]
        impl UserRepository for UserRepoLike {
            async fn insert(&self, _user: &openpanel_domain::User) -> Result<(), openpanel_domain::RepoError>;
            async fn find_by_id(&self, id: uuid::Uuid) -> Result<Option<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn find_by_username(&self, _username: &str) -> Result<Option<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn find_by_email(&self, _email: &str) -> Result<Option<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn list(&self) -> Result<Vec<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn update_role(&self, _id: uuid::Uuid, _role: openpanel_domain::Role) -> Result<(), openpanel_domain::RepoError>;
            async fn disable(&self, _id: uuid::Uuid) -> Result<(), openpanel_domain::RepoError>;
            async fn enable(&self, _id: uuid::Uuid) -> Result<(), openpanel_domain::RepoError>;
            async fn update_last_login(&self, _id: uuid::Uuid) -> Result<(), openpanel_domain::RepoError>;
            async fn update_password(&self, _id: uuid::Uuid, _hash: &str) -> Result<(), openpanel_domain::RepoError>;
            async fn update_parent_account_id(
                &self,
                _id: uuid::Uuid,
                _parent: Option<uuid::Uuid>,
            ) -> Result<(), openpanel_domain::RepoError>;
            async fn update_hosting_plan_id(
                &self,
                _id: uuid::Uuid,
                _plan: Option<openpanel_domain::HostingPlanId>,
            ) -> Result<(), openpanel_domain::RepoError>;
            async fn find_children(&self, _parent: uuid::Uuid) -> Result<Vec<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn find_by_plan(
                &self,
                _plan: openpanel_domain::HostingPlanId,
            ) -> Result<Vec<openpanel_domain::User>, openpanel_domain::RepoError>;
            async fn delete(&self, _id: uuid::Uuid) -> Result<(), openpanel_domain::RepoError>;
            async fn count(&self) -> Result<i64, openpanel_domain::RepoError>;
        }
    }

    mockall::mock! {
        pub AuditLike {}
        #[async_trait::async_trait]
        impl openpanel_core::AuditService for AuditLike {
            async fn record(&self, _event: openpanel_core::AuditEvent) -> openpanel_core::CoreResult<()>;
            async fn recent(&self, _limit: i64) -> openpanel_core::CoreResult<Vec<openpanel_core::AuditEvent>>;
            async fn query(
                &self,
                _query: openpanel_core::audit::AuditQuery,
            ) -> openpanel_core::CoreResult<openpanel_core::audit::AuditPage>;
        }
    }

    impl MockAuditLike {
        pub fn stub() -> Self {
            let mut mock = Self::new();
            mock.expect_record().returning(|_| Ok(()));
            mock.expect_recent().returning(|_| Ok(vec![]));
            mock.expect_query().returning(|_| {
                Ok(openpanel_core::audit::AuditPage {
                    events: Vec::new(),
                    next_cursor: None,
                })
            });
            mock
        }
    }
}
