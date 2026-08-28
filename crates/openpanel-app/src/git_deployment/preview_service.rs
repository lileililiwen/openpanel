//! Preview deployment service: upsert/build/destroy preview
//! environments, per-repo cap enforcement, and TTL reaping.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    DeployRepo, PreviewEnvironment, PreviewError, PreviewRepository, Role, User,
};
use uuid::Uuid;

use crate::git_deployment::SqlitePreviewRepository;

/// Preview service: manages per-PR throwaway environments.
pub struct PreviewService {
    repo: Arc<SqlitePreviewRepository>,
    audit: Arc<dyn AuditService>,
    base_domain: String,
    ttl_hours: u32,
    max_per_repo: u32,
}

impl PreviewService {
    /// Construct a preview service.
    pub fn new(
        repo: Arc<SqlitePreviewRepository>,
        audit: Arc<dyn AuditService>,
        base_domain: impl Into<String>,
        ttl_hours: u32,
        max_per_repo: u32,
    ) -> Self {
        Self {
            repo,
            audit,
            base_domain: base_domain.into(),
            ttl_hours,
            max_per_repo,
        }
    }

    /// Upsert a preview for a PR: create if absent, rebuild if it
    /// already exists. Enforces the per-repo cap by evicting the
    /// oldest expired preview first, else rejecting with
    /// `PreviewError::CapReached`.
    pub async fn upsert(
        &self,
        caller: &User,
        repo: &DeployRepo,
        pr: u32,
    ) -> Result<PreviewEnvironment, PreviewError> {
        require_admin(caller)?;
        let now = Utc::now();

        // Cap enforcement: count live previews for the repo.
        let live = self.repo.list_live(repo.id).await.map_err(map_repo)?;
        let existing = live.iter().find(|p| p.pr_number() == pr).cloned();
        if existing.is_none() && live.len() as u32 >= self.max_per_repo {
            // Try to evict the oldest expired preview first.
            let expired = self.repo.list_expired(now).await.map_err(map_repo)?;
            let mut evicted = false;
            for p in expired {
                if p.repo_id() == repo.id {
                    self.destroy_inner(&p, "cap_evict").await?;
                    evicted = true;
                    break;
                }
            }
            if !evicted {
                return Err(PreviewError::CapReached);
            }
        }

        let mut preview = match existing {
            Some(p) => p,
            None => PreviewEnvironment::new(
                Uuid::new_v4(),
                repo.site_id,
                repo.id,
                pr,
                &self.base_domain,
                now,
            )
            .map_err(map_domain)?,
        };

        // Mock builder: Creating -> Building -> Ready (or Ready -> Building for rebuild).
        if preview.state() == openpanel_domain::PreviewState::Ready {
            preview.rebuild(now).map_err(map_domain)?;
        } else {
            preview.start_building(now).map_err(map_domain)?;
        }
        preview
            .mark_ready(now, self.ttl_hours)
            .map_err(map_domain)?;
        self.repo.save(&preview).await.map_err(map_repo)?;

        self.audit(
            caller,
            AuditAction::PreviewCreated,
            preview.id(),
            serde_json::json!({
                "site_id": repo.site_id,
                "pr": pr,
                "hostname": preview.hostname(),
            }),
        )
        .await;

        Ok(preview)
    }

    /// List previews for a site.
    pub async fn list(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Vec<PreviewEnvironment>, PreviewError> {
        require_admin(caller)?;
        self.repo.list_for_site(site_id).await.map_err(map_repo)
    }

    /// Destroy a preview for a PR on a site.
    pub async fn destroy(&self, caller: &User, site_id: Uuid, pr: u32) -> Result<(), PreviewError> {
        require_admin(caller)?;
        let previews = self.repo.list_for_site(site_id).await.map_err(map_repo)?;
        let preview = previews
            .iter()
            .find(|p| p.pr_number() == pr && p.state() != openpanel_domain::PreviewState::Destroyed)
            .ok_or(PreviewError::Invalid("preview not found".into()))?;
        self.destroy_inner(preview, "manual").await
    }

    /// Redeploy an existing preview (rebuild from the current branch).
    pub async fn redeploy(
        &self,
        caller: &User,
        site_id: Uuid,
        pr: u32,
    ) -> Result<PreviewEnvironment, PreviewError> {
        require_admin(caller)?;
        let previews = self.repo.list_for_site(site_id).await.map_err(map_repo)?;
        let mut preview = previews
            .iter()
            .find(|p| p.pr_number() == pr && p.state() != openpanel_domain::PreviewState::Destroyed)
            .cloned()
            .ok_or(PreviewError::Invalid("preview not found".into()))?;
        let now = Utc::now();
        preview.start_building(now).map_err(map_domain)?;
        preview
            .mark_ready(now, self.ttl_hours)
            .map_err(map_domain)?;
        self.repo.save(&preview).await.map_err(map_repo)?;
        Ok(preview)
    }

    /// Reap expired previews. Returns the number destroyed.
    pub async fn reap_expired(&self) -> Result<usize, PreviewError> {
        let now = Utc::now();
        let expired = self.repo.list_expired(now).await.map_err(map_repo)?;
        let mut count = 0;
        for p in expired {
            self.destroy_inner(&p, "expired").await?;
            count += 1;
        }
        Ok(count)
    }

    async fn destroy_inner(
        &self,
        preview: &PreviewEnvironment,
        reason: &str,
    ) -> Result<(), PreviewError> {
        let now = Utc::now();
        let mut p = preview.clone();
        p.destroy(now, reason).map_err(map_domain)?;
        self.repo.save(&p).await.map_err(map_repo)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    "system",
                    AuditAction::PreviewDestroyed,
                    AuditOutcome::Success,
                )
                .target(p.id().to_string())
                .metadata(serde_json::json!({ "reason": reason })),
            )
            .await;
        Ok(())
    }

    async fn audit(
        &self,
        caller: &User,
        action: AuditAction,
        target: Uuid,
        metadata: serde_json::Value,
    ) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(caller.username().as_str(), action, AuditOutcome::Success)
                    .target(target.to_string())
                    .metadata(metadata),
            )
            .await;
    }
}

fn require_admin(caller: &User) -> Result<(), PreviewError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(PreviewError::Invalid("forbidden".into())),
    }
}

fn map_repo(e: openpanel_domain::RepoError) -> PreviewError {
    PreviewError::Invalid(e.to_string())
}

fn map_domain(e: PreviewError) -> PreviewError {
    e
}
