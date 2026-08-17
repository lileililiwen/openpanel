//! Quotas application service: policy CRUD, sample ingestion, and
//! the soft/hard threshold dispatcher.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    QuotaError, QuotaPolicy, QuotaRepository, QuotaSubject, QuotaUsage, UserRepository,
    identity::repository::UserRepository as _,
};
use uuid::Uuid;

use crate::quotas::repo::SqliteQuotaRepository;

/// Quotas application service.
#[derive(Clone)]
pub struct QuotaService {
    repo: Arc<dyn QuotaRepository>,
    users: Arc<dyn UserRepository>,
    audit: Arc<dyn AuditService>,
}

impl QuotaService {
    /// Build a service over the given repositories.
    pub fn new(
        repo: Arc<dyn QuotaRepository>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            users,
            audit,
        }
    }

    /// Build a service backed by the SQLite adapter.
    pub fn with_sqlite(
        pool: sqlx::Pool<sqlx::Sqlite>,
        users: Arc<dyn UserRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self::new(Arc::new(SqliteQuotaRepository::new(pool)), users, audit)
    }

    /// Find a policy by subject.
    pub async fn find_by_subject(
        &self,
        subject_kind: QuotaSubject,
        subject_id: Uuid,
    ) -> Result<Option<QuotaPolicy>, QuotaError> {
        self.repo.find_by_subject(subject_kind, subject_id).await
    }

    /// Find a policy by id.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<QuotaPolicy>, QuotaError> {
        self.repo.find_by_id(id).await
    }

    /// List all policies.
    pub async fn list(&self) -> Result<Vec<QuotaPolicy>, QuotaError> {
        self.repo.list().await
    }

    /// Set (or replace) a user's policy. The application layer
    /// merges the new policy with any existing one in the same
    /// transaction in the follow-on persistence change.
    pub async fn set_user_policy(
        &self,
        user_id: Uuid,
        policy: QuotaPolicy,
        actor: &str,
    ) -> Result<QuotaPolicy, QuotaError> {
        let user = self
            .users
            .find_by_id(user_id)
            .await
            .map_err(|e| QuotaError::Persistence(e.0))?;
        if user.is_none() {
            return Err(QuotaError::SubjectNotFound);
        }
        let existing = self.repo.find_by_subject(QuotaSubject::User, user_id).await?;
        match existing {
            Some(_) => self.repo.update(&policy).await?,
            None => self.repo.insert(&policy).await?,
        }
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::QuotaPolicyChanged, AuditOutcome::Success)
                    .target(policy.id().to_string())
                    .metadata(serde_json::json!({
                        "subject_kind": "user",
                        "subject_id": user_id.to_string(),
                    })),
            )
            .await
            .ok();
        Ok(policy)
    }

    /// Delete a policy.
    pub async fn delete(
        &self,
        id: Uuid,
        actor: &str,
    ) -> Result<(), QuotaError> {
        self.repo.delete(id).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::QuotaPolicyDeleted, AuditOutcome::Success)
                    .target(id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Ingest a usage sample. The service computes the
    /// soft/hard threshold sets and persists the row.
    pub async fn sample(
        &self,
        policy: QuotaPolicy,
        disk_used_bytes: u64,
        disk_inodes_used: u64,
        bandwidth_used_bytes: u64,
        actor: &str,
    ) -> Result<QuotaUsage, QuotaError> {
        let now = Utc::now();
        let usage = QuotaUsage::sample(
            policy.subject_id(),
            &policy,
            disk_used_bytes,
            disk_inodes_used,
            bandwidth_used_bytes,
            now,
        );
        self.repo.insert_usage(&usage).await?;
        if !usage.over_hard.is_empty() {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::QuotaHardLimitReached, AuditOutcome::Failure)
                        .target(policy.subject_id().to_string())
                        .metadata(serde_json::json!({
                            "over_hard": usage.over_hard.iter().map(|d| match d {
                                openpanel_domain::quotas::QuotaDimension::Disk => "disk",
                                openpanel_domain::quotas::QuotaDimension::Bandwidth => "bandwidth",
                                openpanel_domain::quotas::QuotaDimension::Inodes => "inodes",
                            }).collect::<Vec<_>>(),
                        })),
                )
                .await
                .ok();
        } else if !usage.over_soft.is_empty() {
            self.audit
                .record(
                    AuditEvent::new(actor, AuditAction::QuotaSoftLimitReached, AuditOutcome::Success)
                        .target(policy.subject_id().to_string())
                        .metadata(serde_json::json!({
                            "over_soft": usage.over_soft.iter().map(|d| match d {
                                openpanel_domain::quotas::QuotaDimension::Disk => "disk",
                                openpanel_domain::quotas::QuotaDimension::Bandwidth => "bandwidth",
                                openpanel_domain::quotas::QuotaDimension::Inodes => "inodes",
                            }).collect::<Vec<_>>(),
                        })),
                )
                .await
                .ok();
        }
        Ok(usage)
    }

    /// Most recent sample for a subject.
    pub async fn latest_usage(
        &self,
        subject_id: Uuid,
    ) -> Result<Option<QuotaUsage>, QuotaError> {
        self.repo.latest_usage(subject_id).await
    }
}
