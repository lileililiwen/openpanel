//! Load balancing services: member rotator and the probe pipeline.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    HealthProbe, LbRepository, LbStatus, Member, Pool, ProbeDecision, Role, User, next_member,
};
use uuid::Uuid;

use crate::load_balancing::SqliteLbRepository;

/// Recording health probe used by tests. Each call records its
/// address; the configured success / failure table is consulted
/// to return a deterministic outcome.
pub struct RecordingHealthProbe {
    table: std::sync::Mutex<std::collections::HashMap<String, bool>>,
    calls: std::sync::Mutex<Vec<String>>,
}

impl RecordingHealthProbe {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            table: std::sync::Mutex::new(std::collections::HashMap::new()),
            calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Configure the next probe outcome for `address`.
    pub fn set(&self, address: &str, ok: bool) {
        #[allow(clippy::expect_used)]
        self.table
            .lock()
            .expect("table")
            .insert(address.to_string(), ok);
    }

    /// Snapshot recorded calls.
    pub fn calls(&self) -> Vec<String> {
        #[allow(clippy::expect_used)]
        self.calls.lock().expect("calls").clone()
    }
}

impl Default for RecordingHealthProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HealthProbe for RecordingHealthProbe {
    async fn probe(&self, address: &str) -> bool {
        #[allow(clippy::expect_used)]
        self.calls.lock().expect("calls").push(address.to_string());
        #[allow(clippy::expect_used)]
        self.table
            .lock()
            .expect("table")
            .get(address)
            .copied()
            .unwrap_or(true)
    }
}

/// Outcome of a rotation request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationDecision {
    /// Member picked by the rotator, or `None` when the pool is
    /// empty / all members are ineligible.
    pub member: Option<Member>,
}

/// Member rotator: picks the next eligible member for a pool and
/// updates the pool's state in response to probe outcomes.
pub struct MemberRotator {
    repo: Arc<SqliteLbRepository>,
    audit: Arc<dyn AuditService>,
}

impl MemberRotator {
    /// Construct a rotator.
    pub fn new(repo: Arc<SqliteLbRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Pick the next eligible member for `pool_id`.
    pub async fn next(&self, pool_id: Uuid) -> Result<RotationDecision, openpanel_domain::LbError> {
        let pool = self
            .repo
            .get_pool(pool_id)
            .await?
            .ok_or(openpanel_domain::LbError::PoolNotFound(pool_id))?;
        let members = self.repo.list_members(pool_id).await?;
        let next = next_member(&members, pool.algorithm).cloned();
        Ok(RotationDecision { member: next })
    }

    /// Apply a probe outcome for `member_id` and persist the new
    /// status. Caller MUST be Admin/Owner.
    pub async fn apply_probe(
        &self,
        caller: &User,
        member_id: Uuid,
        ok: bool,
        failure_threshold: u32,
        recovery_threshold: u32,
    ) -> Result<ProbeDecision, openpanel_domain::LbError> {
        require_admin(caller)?;
        let mut member = self
            .repo
            .get_member(member_id)
            .await?
            .ok_or(openpanel_domain::LbError::MemberNotFound(member_id))?;
        let decision = ProbeDecision::evaluate(&member, ok, failure_threshold, recovery_threshold);
        member.status = decision.new_status;
        if !ok {
            member.failed_probe_count = member.failed_probe_count.saturating_add(1);
        } else if decision.new_status == LbStatus::Healthy {
            member.failed_probe_count = 0;
        }
        member.last_probe_at = Some(Utc::now());
        self.repo.save_member(&member).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::LbMemberChanged,
                    AuditOutcome::Success,
                )
                .target(member_id.to_string())
                .metadata(serde_json::json!({
                    "ok": ok,
                    "new_status": member.status.as_str(),
                })),
            )
            .await;
        Ok(decision)
    }
}

/// Top-level façade.
pub struct LbService {
    repo: Arc<SqliteLbRepository>,
    rotator: MemberRotator,
}

impl LbService {
    /// Construct the façade.
    pub fn new(repo: Arc<SqliteLbRepository>, rotator: MemberRotator) -> Self {
        Self { repo, rotator }
    }

    /// Create a pool.
    pub async fn create_pool(
        &self,
        caller: &User,
        pool: Pool,
    ) -> Result<Pool, openpanel_domain::LbError> {
        require_admin(caller)?;
        self.repo.save_pool(&pool).await?;
        Ok(pool)
    }

    /// Add a member to a pool.
    pub async fn add_member(
        &self,
        caller: &User,
        member: Member,
    ) -> Result<Member, openpanel_domain::LbError> {
        require_admin(caller)?;
        member.validate()?;
        self.repo.save_member(&member).await?;
        Ok(member)
    }

    /// List pools.
    pub async fn list_pools(&self) -> Result<Vec<Pool>, openpanel_domain::LbError> {
        Ok(self.repo.list_pools().await?)
    }

    /// List members for a pool.
    pub async fn list_members(
        &self,
        pool_id: Uuid,
    ) -> Result<Vec<Member>, openpanel_domain::LbError> {
        Ok(self.repo.list_members(pool_id).await?)
    }

    /// Pick the next member.
    pub async fn next(&self, pool_id: Uuid) -> Result<RotationDecision, openpanel_domain::LbError> {
        self.rotator.next(pool_id).await
    }

    /// Apply a probe outcome.
    pub async fn apply_probe(
        &self,
        caller: &User,
        member_id: Uuid,
        ok: bool,
        failure_threshold: u32,
        recovery_threshold: u32,
    ) -> Result<ProbeDecision, openpanel_domain::LbError> {
        self.rotator
            .apply_probe(caller, member_id, ok, failure_threshold, recovery_threshold)
            .await
    }
}

fn require_admin(caller: &User) -> Result<(), openpanel_domain::LbError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(openpanel_domain::LbError::Forbidden),
    }
}
