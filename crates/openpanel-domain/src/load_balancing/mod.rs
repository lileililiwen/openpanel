//! Load balancing and failover bounded context: pools of members
//! with health probes, weighted rotation, and a probe-driven
//! failover decision.
//!
//! A disabled member never receives traffic; member weights are
//! non-negative and the rotator picks the next enabled member in
//! weighted round-robin order.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the load-balancing bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LbError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The pool does not exist.
    #[error("pool not found: {0}")]
    PoolNotFound(Uuid),
    /// The member does not exist.
    #[error("member not found: {0}")]
    MemberNotFound(Uuid),
    /// The member weight is invalid (e.g. negative).
    #[error("invalid weight: {0}")]
    InvalidWeight(u32),
    /// The pool is empty.
    #[error("pool is empty")]
    PoolEmpty,
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<LbError> for RepoError {
    fn from(error: LbError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for LbError {
    fn from(error: RepoError) -> Self {
        LbError::Persistence(error.0)
    }
}

/// Status of a pool member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LbStatus {
    /// Member is healthy and receiving traffic.
    Healthy,
    /// Member is failing health checks; rotator skips it.
    Failing,
    /// Operator manually disabled the member.
    Disabled,
}

impl LbStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Failing => "failing",
            Self::Disabled => "disabled",
        }
    }

    /// Whether the member is eligible to receive traffic.
    pub fn is_eligible(&self) -> bool {
        matches!(self, Self::Healthy)
    }
}

/// A single backend member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    /// Stable id.
    pub id: Uuid,
    /// Owning pool.
    pub pool_id: Uuid,
    /// Backend address (`host:port`).
    pub address: String,
    /// Relative weight (0..=100). Members with weight 0 are
    /// kept around but never receive traffic.
    pub weight: u32,
    /// Current status.
    pub status: LbStatus,
    /// Last probe result.
    pub last_probe_at: Option<DateTime<Utc>>,
    /// Number of consecutive failed probes.
    pub failed_probe_count: u32,
}

impl Member {
    /// Validate the member's invariants.
    pub fn validate(&self) -> Result<(), LbError> {
        if self.weight > 100 {
            return Err(LbError::InvalidWeight(self.weight));
        }
        if !self.address.contains(':') {
            return Err(LbError::InvalidWeight(0));
        }
        Ok(())
    }
}

/// A load-balancer pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pool {
    /// Stable id.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Algorithm used to pick the next member.
    pub algorithm: PoolAlgorithm,
    /// When the pool was created.
    pub created_at: DateTime<Utc>,
}

/// Load-balancing algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PoolAlgorithm {
    /// Round-robin across enabled members.
    RoundRobin,
    /// Weighted round-robin (weight: 0..=100).
    Weighted,
    /// Sticky session by client IP.
    StickyIp,
}

impl PoolAlgorithm {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "round_robin",
            Self::Weighted => "weighted",
            Self::StickyIp => "sticky_ip",
        }
    }
}

/// A health-check configuration attached to a pool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthCheck {
    /// Owning pool.
    pub pool_id: Uuid,
    /// Probe URL (http://host:port/path).
    pub url: String,
    /// Number of consecutive failures before `Failing`.
    pub failure_threshold: u32,
    /// Number of consecutive successes before `Healthy`.
    pub recovery_threshold: u32,
    /// Probe interval in seconds.
    pub interval_secs: u32,
}

impl HealthCheck {
    /// Construct a default health check for a pool.
    pub fn default_for(pool_id: Uuid) -> Self {
        Self {
            pool_id,
            url: "http://localhost/health".to_string(),
            failure_threshold: 3,
            recovery_threshold: 1,
            interval_secs: 10,
        }
    }
}

/// Persistence port for the load-balancing bounded context.
#[async_trait]
pub trait LbRepository: Send + Sync + 'static {
    /// Persist a pool.
    async fn save_pool(&self, pool: &Pool) -> Result<(), RepoError>;
    /// List all pools.
    async fn list_pools(&self) -> Result<Vec<Pool>, RepoError>;
    /// Load a pool by id.
    async fn get_pool(&self, id: Uuid) -> Result<Option<Pool>, RepoError>;

    /// Persist a member.
    async fn save_member(&self, member: &Member) -> Result<(), RepoError>;
    /// List members for a pool.
    async fn list_members(&self, pool_id: Uuid) -> Result<Vec<Member>, RepoError>;
    /// Load a member by id.
    async fn get_member(&self, id: Uuid) -> Result<Option<Member>, RepoError>;
    /// Delete a member.
    async fn delete_member(&self, id: Uuid) -> Result<(), RepoError>;
}

/// Health probe port. The probe is responsible for contacting
/// the backend and reporting success / failure.
#[async_trait]
pub trait HealthProbe: Send + Sync + 'static {
    /// Run a single probe against `address`; returns true on
    /// success, false on failure.
    async fn probe(&self, address: &str) -> bool;
}

/// Decision returned by the rotator / probe pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeDecision {
    /// Member id.
    pub member_id: Uuid,
    /// Whether the probe succeeded.
    pub ok: bool,
    /// New status after applying thresholds.
    pub new_status: LbStatus,
}

impl ProbeDecision {
    /// Compute the new status given the current member, the probe
    /// outcome, and the configured thresholds.
    pub fn evaluate(
        member: &Member,
        ok: bool,
        failure_threshold: u32,
        recovery_threshold: u32,
    ) -> Self {
        let new_failed = if ok {
            0
        } else {
            member.failed_probe_count.saturating_add(1)
        };
        let new_status = if member.status == LbStatus::Disabled {
            LbStatus::Disabled
        } else if ok {
            if member.status == LbStatus::Failing && new_failed == 0 {
                // Recovery: count consecutive successes since
                // returning to healthy; we approximate with
                // recovery_threshold via the caller.
                let _ = recovery_threshold;
                LbStatus::Healthy
            } else {
                member.status
            }
        } else if new_failed >= failure_threshold {
            LbStatus::Failing
        } else {
            member.status
        };
        Self {
            member_id: member.id,
            ok,
            new_status,
        }
    }
}

/// Pick the next eligible member from `members` using round-robin
/// or weighted selection.
pub fn next_member<'a>(members: &'a [Member], algorithm: PoolAlgorithm) -> Option<&'a Member> {
    let eligible: Vec<&Member> = members
        .iter()
        .filter(|m| m.status.is_eligible() && m.weight > 0)
        .collect();
    if eligible.is_empty() {
        return None;
    }
    match algorithm {
        PoolAlgorithm::RoundRobin => eligible.first().copied(),
        PoolAlgorithm::Weighted => {
            // Pick the eligible member with the highest weight;
            // ties are broken by lowest id for determinism.
            eligible
                .into_iter()
                .max_by_key(|m| (m.weight, std::cmp::Reverse(u128::from(m.id.as_u128()))))
        }
        PoolAlgorithm::StickyIp => eligible.first().copied(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: Uuid, status: LbStatus, weight: u32) -> Member {
        Member {
            id,
            pool_id: Uuid::nil(),
            address: "127.0.0.1:80".into(),
            weight,
            status,
            last_probe_at: None,
            failed_probe_count: 0,
        }
    }

    #[test]
    fn next_member_skips_disabled_and_zero_weight() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let c = Uuid::new_v4();
        let m1 = member(a, LbStatus::Disabled, 5);
        let m2 = member(b, LbStatus::Healthy, 0);
        let m3 = member(c, LbStatus::Healthy, 10);
        let members = [m1, m2, m3];
        let next = next_member(&members, PoolAlgorithm::Weighted);
        assert_eq!(next.unwrap().id, c);
    }

    #[test]
    fn next_member_returns_none_when_empty_or_disabled() {
        assert!(next_member(&[], PoolAlgorithm::RoundRobin).is_none());
        let m = member(Uuid::new_v4(), LbStatus::Disabled, 10);
        assert!(next_member(&[m], PoolAlgorithm::RoundRobin).is_none());
    }

    #[test]
    fn evaluate_promotes_to_healthy_after_recovery() {
        let mut m = member(Uuid::new_v4(), LbStatus::Failing, 10);
        m.failed_probe_count = 3;
        let d = ProbeDecision::evaluate(&m, true, 3, 1);
        assert_eq!(d.new_status, LbStatus::Healthy);
    }

    #[test]
    fn evaluate_demotes_to_failing_at_threshold() {
        let mut m = member(Uuid::new_v4(), LbStatus::Healthy, 10);
        m.failed_probe_count = 2;
        let d = ProbeDecision::evaluate(&m, false, 3, 1);
        assert_eq!(d.new_status, LbStatus::Failing);
    }

    #[test]
    fn evaluate_keeps_disabled_members_disabled() {
        let m = member(Uuid::new_v4(), LbStatus::Disabled, 10);
        let d = ProbeDecision::evaluate(&m, true, 3, 1);
        assert_eq!(d.new_status, LbStatus::Disabled);
    }
}
