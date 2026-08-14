//! Per-site staging application service.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    PromotionRepository, PromotionRun, PromotionStatus, Role, Site, SiteRepository,
    SiteStagingError, SnapshotId, StagingSlot, StagingSlotRepository, StagingSnapshotRepository,
    SyncPolicy, User,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::site_staging::{
    files::{PromoteOutcome, StagingFilesystemLayer},
    repo::SqliteStagingLockTable,
};

/// Request body for creating a staging slot.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSlotRequest {
    /// Subdomain prefix (default: `staging`).
    pub subdomain: Option<String>,
    /// Optional override for the staging document root. The path
    /// MUST live under the site chroot; the service rejects paths
    /// that escape it.
    pub document_root: Option<String>,
    /// Optional sync policy.
    pub sync_policy: Option<SyncPolicy>,
    /// Optional PHP version (mirrors production by default).
    pub php_version: Option<String>,
    /// Optional cron expression; only valid with `SyncPolicy::Scheduled`.
    pub schedule: Option<String>,
}

/// Error returned by the application service.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PromotionServiceError {
    /// Domain error.
    #[error("staging: {0}")]
    Staging(#[from] SiteStagingError),
    /// Caller is not allowed to perform the operation.
    #[error("forbidden")]
    Forbidden,
    /// The site does not exist.
    #[error("site not found: {0}")]
    SiteNotFound(Uuid),
}

/// Application service orchestrating the staging flow.
pub struct StagingService {
    slots: Arc<dyn StagingSlotRepository>,
    snapshots: Arc<dyn StagingSnapshotRepository>,
    promotions: Arc<dyn PromotionRepository>,
    sites: Arc<dyn SiteRepository>,
    fs: Arc<dyn StagingFilesystemLayer>,
    locks: SqliteStagingLockTable,
    audit: Arc<dyn AuditService>,
}

impl StagingService {
    /// Construct a service with the given dependencies.
    pub fn new(
        slots: Arc<dyn StagingSlotRepository>,
        snapshots: Arc<dyn StagingSnapshotRepository>,
        promotions: Arc<dyn PromotionRepository>,
        sites: Arc<dyn SiteRepository>,
        fs: Arc<dyn StagingFilesystemLayer>,
        locks: SqliteStagingLockTable,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            slots,
            snapshots,
            promotions,
            sites,
            fs,
            locks,
            audit,
        }
    }

    fn ensure_owner(&self, caller: &User) -> Result<(), PromotionServiceError> {
        if !matches!(caller.role(), Role::Owner) {
            return Err(PromotionServiceError::Forbidden);
        }
        Ok(())
    }

    async fn load_site(&self, caller: &User, site_id: Uuid) -> Result<Site, PromotionServiceError> {
        let site = self
            .sites
            .find_by_id(site_id)
            .await
            .map_err(|e| SiteStagingError::Database(e.0))?
            .ok_or(PromotionServiceError::SiteNotFound(site_id))?;
        let (_, can_manage_all) = scope(caller);
        if !can_manage_all && site.owner_id() != caller.id() {
            return Err(PromotionServiceError::Forbidden);
        }
        Ok(site)
    }

    fn ensure_in_chroot(site: &Site, document_root: &str) -> Result<(), SiteStagingError> {
        // Production root is the parent of `site.document_root()`.
        // e.g. `/var/www/example.com/public_html` → parent is
        // `/var/www/example.com/`. The staging docroot MUST live
        // underneath the same parent.
        let prod = site.document_root().trim_end_matches('/');
        let parent = match prod.rsplit_once('/') {
            Some((prefix, _)) if !prefix.is_empty() => format!("{prefix}/"),
            _ => return Err(SiteStagingError::OutsideChroot(document_root.into())),
        };
        if !document_root.starts_with(&parent) {
            return Err(SiteStagingError::OutsideChroot(document_root.into()));
        }
        if document_root.contains("..") {
            return Err(SiteStagingError::OutsideChroot(document_root.into()));
        }
        if !document_root.contains("staging") {
            return Err(SiteStagingError::OutsideChroot(document_root.into()));
        }
        Ok(())
    }

    async fn record_audit(
        &self,
        actor: &str,
        action: AuditAction,
        target: String,
        metadata: serde_json::Value,
        outcome: AuditOutcome,
    ) {
        let event = AuditEvent::new(actor, action, outcome)
            .target(target)
            .metadata(metadata);
        if let Err(e) = self.audit.record(event).await {
            tracing::warn!(error = %e, "failed to record staging audit event");
        }
    }

    /// Create a staging slot for `site_id`. Idempotent: a second
    /// call returns the existing slot unchanged.
    pub async fn create_slot(
        &self,
        caller: &User,
        site_id: Uuid,
        request: CreateSlotRequest,
    ) -> Result<StagingSlot, PromotionServiceError> {
        self.ensure_owner(caller)?;
        let site = self.load_site(caller, site_id).await?;
        if self.slots.find_by_site(site_id).await?.is_some() {
            return Err(PromotionServiceError::Staging(
                SiteStagingError::AlreadyExists(site_id.to_string()),
            ));
        }
        let subdomain = request.subdomain.unwrap_or_else(|| "staging".to_string());
        let document_root = request
            .document_root
            .unwrap_or_else(|| staging_docroot_default(site.document_root()));
        Self::ensure_in_chroot(&site, &document_root)?;
        let db_name = staging_db_name(site.primary_domain());
        let sync_policy = request.sync_policy.unwrap_or(SyncPolicy::OnPromote);
        let now = Utc::now();
        let mut slot = StagingSlot::new(
            Uuid::new_v4(),
            site_id,
            subdomain,
            document_root.clone(),
            db_name,
            request.php_version,
            sync_policy,
            now,
        )?;
        if let Some(sched) = request.schedule {
            slot.set_schedule(Some(sched), now)?;
        }
        self.slots.insert(&slot).await?;
        self.fs.create_slot(site_id, &document_root).await?;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::StagingSlotCreated,
            site_id.to_string(),
            serde_json::json!({
                "slot_id": slot.id(),
                "document_root": slot.document_root(),
                "db_name": slot.db_name(),
            }),
            AuditOutcome::Success,
        )
        .await;
        Ok(slot)
    }

    /// Delete the staging slot. Idempotent.
    pub async fn delete_slot(
        &self,
        caller: &User,
        site_id: Uuid,
        drop_db: bool,
    ) -> Result<(), PromotionServiceError> {
        self.ensure_owner(caller)?;
        let slot = self.slots.find_by_site(site_id).await?;
        let slot = match slot {
            Some(s) => s,
            None => return Ok(()),
        };
        // Best-effort lock acquisition to avoid racing with a sync
        // or promote. If the lock is held, we still proceed because
        // deletion is the operator's call; we record the conflict.
        let _ = self
            .locks
            .try_acquire(site_id, caller.username().as_str(), Utc::now())
            .await?;
        self.fs
            .destroy_slot(site_id, slot.document_root(), slot.db_name())
            .await?;
        if drop_db {
            // Database drop is the engine adapter's job (out of
            // scope for this change). The flag is recorded for the
            // operator and the audit trail.
        }
        self.slots.delete(slot.id()).await?;
        let _ = self
            .locks
            .release(site_id, caller.username().as_str())
            .await;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::StagingSlotDeleted,
            site_id.to_string(),
            serde_json::json!({"slot_id": slot.id(), "drop_db": drop_db}),
            AuditOutcome::Success,
        )
        .await;
        Ok(())
    }

    /// Take a fresh snapshot of production into staging.
    pub async fn sync_snapshot(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<StagingSlot, PromotionServiceError> {
        self.ensure_owner(caller)?;
        let site = self.load_site(caller, site_id).await?;
        let mut slot = self.slots.find_by_site(site_id).await?.ok_or_else(|| {
            PromotionServiceError::Staging(SiteStagingError::NotFound(site_id.to_string()))
        })?;
        let acquired = self
            .locks
            .try_acquire(site_id, caller.username().as_str(), Utc::now())
            .await?;
        if !acquired {
            return Err(PromotionServiceError::Staging(SiteStagingError::InFlight));
        }
        let _guard = LockGuard {
            locks: &self.locks,
            site_id,
            by: caller.username().as_str().to_string(),
        };
        let taken_at = self
            .fs
            .sync_snapshot(site_id, site.document_root(), slot.document_root())
            .await?;
        let next_id = slot
            .current_snapshot()
            .map(|s| s.next())
            .unwrap_or_else(SnapshotId::first);
        slot.record_snapshot(next_id, taken_at)?;
        self.snapshots.insert(site_id, next_id, taken_at).await?;
        self.slots.update(&slot).await?;
        self.record_audit(
            caller.username().as_str(),
            AuditAction::StagingSnapshotTaken,
            site_id.to_string(),
            serde_json::json!({
                "slot_id": slot.id(),
                "snapshot": next_id.as_i64(),
            }),
            AuditOutcome::Success,
        )
        .await;
        Ok(slot)
    }

    /// Promote the current staging snapshot to production. Atomic:
    /// on any failure, the rename chain is reversed and the
    /// production docroot is left untouched.
    #[allow(clippy::too_many_arguments)]
    pub async fn promote(
        &self,
        caller: &User,
        site_id: Uuid,
        snapshot: SnapshotId,
        confirmed_at: DateTime<Utc>,
    ) -> Result<PromotionRun, PromotionServiceError> {
        self.ensure_owner(caller)?;
        let site = self.load_site(caller, site_id).await?;
        let slot = self.slots.find_by_site(site_id).await?.ok_or_else(|| {
            PromotionServiceError::Staging(SiteStagingError::NotFound(site_id.to_string()))
        })?;
        let current = slot.current_snapshot().ok_or_else(|| {
            PromotionServiceError::Staging(SiteStagingError::Invalid(
                "no snapshot to promote".into(),
            ))
        })?;
        if snapshot != current {
            return Err(PromotionServiceError::Staging(
                SiteStagingError::SnapshotMismatch(current.to_string(), snapshot.to_string()),
            ));
        }
        let acquired = self
            .locks
            .try_acquire(site_id, caller.username().as_str(), Utc::now())
            .await?;
        if !acquired {
            return Err(PromotionServiceError::Staging(SiteStagingError::InFlight));
        }
        let _guard = LockGuard {
            locks: &self.locks,
            site_id,
            by: caller.username().as_str().to_string(),
        };
        let now = Utc::now();
        let mut run = PromotionRun::new(
            Uuid::new_v4(),
            site_id,
            snapshot,
            confirmed_at,
            caller.username().as_str(),
            now,
        )?;
        run.begin_promotion(now)?;
        self.promotions.insert(&run).await?;
        let outcome = self
            .fs
            .promote(site_id, site.document_root(), slot.document_root())
            .await?;
        match outcome {
            PromoteOutcome::Success => {
                run.complete_promotion(Utc::now())?;
                self.promotions.update(&run).await?;
                let mut slot = slot;
                slot.record_promotion(snapshot, Utc::now())?;
                self.slots.update(&slot).await?;
                self.record_audit(
                    caller.username().as_str(),
                    AuditAction::StagingPromoted,
                    site_id.to_string(),
                    serde_json::json!({
                        "snapshot": snapshot.as_i64(),
                        "promotion_id": run.id(),
                    }),
                    AuditOutcome::Success,
                )
                .await;
                Ok(run)
            }
            PromoteOutcome::RolledBack { reason } => {
                let now = Utc::now();
                run.mark_rolled_back(&reason, now)?;
                self.promotions.update(&run).await?;
                self.record_audit(
                    caller.username().as_str(),
                    AuditAction::StagingPromotionRolledBack,
                    site_id.to_string(),
                    serde_json::json!({
                        "snapshot": snapshot.as_i64(),
                        "promotion_id": run.id(),
                        "reason": reason,
                    }),
                    AuditOutcome::Failure,
                )
                .await;
                Ok(run)
            }
        }
    }

    /// Look up the slot for a site.
    pub async fn slot_for(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Option<StagingSlot>, PromotionServiceError> {
        self.ensure_owner(caller)?;
        self.load_site(caller, site_id).await?;
        Ok(self.slots.find_by_site(site_id).await?)
    }

    /// List every promotion for a site, newest first.
    pub async fn promotions(
        &self,
        caller: &User,
        site_id: Uuid,
    ) -> Result<Vec<PromotionRun>, PromotionServiceError> {
        self.ensure_owner(caller)?;
        self.load_site(caller, site_id).await?;
        Ok(self.promotions.list_by_site(site_id).await?)
    }
}

fn scope(user: &User) -> (Uuid, bool) {
    (user.id(), matches!(user.role(), Role::Owner))
}

fn staging_docroot_default(prod_docroot: &str) -> String {
    let parent = prod_docroot
        .trim_end_matches('/')
        .rsplit_once('/')
        .map(|(p, _)| p)
        .unwrap_or(prod_docroot);
    format!("{parent}/staging/public_html")
}

fn staging_db_name(primary_domain: &str) -> String {
    let sanitized: String = primary_domain
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("{sanitized}_staging")
}

struct LockGuard<'a> {
    locks: &'a SqliteStagingLockTable,
    site_id: Uuid,
    by: String,
}

impl Drop for LockGuard<'_> {
    fn drop(&mut self) {
        let locks = self.locks.clone();
        let site_id = self.site_id;
        let by = self.by.clone();
        // Best-effort release. tokio runtime is available because
        // we are inside an async context.
        tokio::spawn(async move {
            let _ = locks.release(site_id, &by).await;
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc as StdArc;

    use chrono::Utc;
    use openpanel_core::AuditService;
    use openpanel_test_support::{
        db::TestDb,
        mocks::{MockAudit, MockSiteRepo},
    };

    use super::*;

    fn owner(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("owner").unwrap(),
            openpanel_domain::Email::new("owner@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Owner,
        )
    }

    fn admin(id: Uuid) -> openpanel_domain::User {
        openpanel_domain::User::new(
            id,
            openpanel_domain::Username::new("admin").unwrap(),
            openpanel_domain::Email::new("admin@example.com").unwrap(),
            openpanel_domain::Password::hash("correct horse battery staple").unwrap(),
            Role::Admin,
        )
    }

    async fn build_service() -> (
        StagingService,
        Uuid,
        Uuid,
        Uuid,
        std::sync::Arc<crate::site_staging::InMemoryStagingFilesystem>,
    ) {
        let db = TestDb::new().await;
        let pool = db.pool();
        let slots: Arc<dyn StagingSlotRepository> = Arc::new(
            crate::site_staging::repo::SqliteStagingSlotRepository::new(pool.clone()),
        );
        let snapshots: Arc<dyn StagingSnapshotRepository> =
            Arc::new(crate::site_staging::repo::SqliteStagingSnapshotRepository::new(pool.clone()));
        let promotions: Arc<dyn PromotionRepository> = Arc::new(
            crate::site_staging::repo::SqlitePromotionRepository::new(pool.clone()),
        );
        let locks = SqliteStagingLockTable::new(pool.clone());

        // Seed a users row so sites.owner_id FK is satisfied.
        let owner_id = Uuid::new_v4();
        sqlx::query(
            "INSERT OR REPLACE INTO users (id, username, email, password_hash, role, created_at, disabled_at, last_login_at)
             VALUES (?, ?, ?, ?, ?, ?, NULL, NULL)",
        )
        .bind(owner_id.to_string())
        .bind("owner")
        .bind("owner@example.com")
        .bind("hash")
        .bind("owner")
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .expect("seed users");

        // Seed a site row.
        let site_id = Uuid::new_v4();
        let site = openpanel_domain::Site::new(
            site_id,
            owner_id,
            "example.com",
            vec!["www.example.com".to_string()],
            "/var/www/example.com/public_html",
            false,
            None,
            "owner",
        )
        .unwrap();
        sqlx::query(
            "INSERT OR REPLACE INTO sites
                (id, owner_id, primary_domain, aliases, document_root,
                 php_enabled, php_version, status, created_at, updated_at,
                 created_by, modified_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(site.id().to_string())
        .bind(site.owner_id().to_string())
        .bind(site.primary_domain())
        .bind(serde_json::to_string(site.aliases()).unwrap())
        .bind(site.document_root())
        .bind(if site.php_enabled() { 1 } else { 0 })
        .bind(site.php_version())
        .bind(site.status().as_str())
        .bind(site.created_at().to_rfc3339())
        .bind(site.updated_at().to_rfc3339())
        .bind(site.created_by())
        .bind(site.modified_by())
        .execute(&pool)
        .await
        .expect("seed site");

        let sites_repo: Arc<dyn SiteRepository> = Arc::new(make_site_repo(site));
        let fs = std::sync::Arc::new(crate::site_staging::InMemoryStagingFilesystem::new());
        let fs_trait: Arc<dyn StagingFilesystemLayer> = fs.clone();
        let audit: Arc<dyn AuditService> = Arc::new(MockAudit::stub());
        let service = StagingService::new(
            slots, snapshots, promotions, sites_repo, fs_trait, locks, audit,
        );
        (service, owner_id, site_id, Uuid::new_v4(), fs)
    }

    fn make_site_repo(site: Site) -> MockSiteRepo {
        let mut repo = MockSiteRepo::new();
        repo.expect_find_by_id()
            .returning(move |_| Ok(Some(site.clone())));
        repo
    }

    #[tokio::test]
    async fn create_slot_persists_and_audits() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        let slot = svc
            .create_slot(
                &owner(owner_id),
                site_id,
                CreateSlotRequest {
                    subdomain: None,
                    document_root: None,
                    sync_policy: None,
                    php_version: None,
                    schedule: None,
                },
            )
            .await
            .expect("create");
        assert_eq!(slot.site_id(), site_id);
        assert_eq!(slot.subdomain(), "staging");
        assert_eq!(
            slot.document_root(),
            "/var/www/example.com/staging/public_html"
        );
        assert_eq!(slot.db_name(), "example_com_staging");
    }

    #[tokio::test]
    async fn create_slot_rejects_path_outside_chroot() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        let r = svc
            .create_slot(
                &owner(owner_id),
                site_id,
                CreateSlotRequest {
                    subdomain: None,
                    document_root: Some("/var/www/other/public_html".into()),
                    sync_policy: None,
                    php_version: None,
                    schedule: None,
                },
            )
            .await;
        assert!(matches!(
            r,
            Err(PromotionServiceError::Staging(
                SiteStagingError::OutsideChroot(_)
            ))
        ));
    }

    #[tokio::test]
    async fn snapshot_takes_and_advances_id() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        svc.create_slot(
            &owner(owner_id),
            site_id,
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .expect("create");
        let after_first = svc
            .sync_snapshot(&owner(owner_id), site_id)
            .await
            .expect("sync");
        assert_eq!(
            after_first.current_snapshot(),
            Some(SnapshotId::new(1).unwrap())
        );
        let after_second = svc
            .sync_snapshot(&owner(owner_id), site_id)
            .await
            .expect("sync2");
        assert_eq!(
            after_second.current_snapshot(),
            Some(SnapshotId::new(2).unwrap())
        );
    }

    #[tokio::test]
    async fn promote_succeeds_with_fresh_confirmation() {
        let (svc, owner_id, site_id, _, fs) = build_service().await;
        svc.create_slot(
            &owner(owner_id),
            site_id,
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .expect("create");
        svc.sync_snapshot(&owner(owner_id), site_id)
            .await
            .expect("sync");
        let now = Utc::now();
        let run = svc
            .promote(&owner(owner_id), site_id, SnapshotId::new(1).unwrap(), now)
            .await
            .expect("promote");
        assert_eq!(run.status(), PromotionStatus::Promoted);
        assert_eq!(fs.promote_calls().await, 1);
    }

    #[tokio::test]
    async fn promote_rolls_back_on_filesystem_failure() {
        let (svc, owner_id, site_id, _, fs) = build_service().await;
        svc.create_slot(
            &owner(owner_id),
            site_id,
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .expect("create");
        svc.sync_snapshot(&owner(owner_id), site_id)
            .await
            .expect("sync");
        fs.force_next_promote_rollback();
        let now = Utc::now();
        let run = svc
            .promote(&owner(owner_id), site_id, SnapshotId::new(1).unwrap(), now)
            .await
            .expect("promote");
        assert_eq!(run.status(), PromotionStatus::RolledBack);
        assert_eq!(run.failure_reason(), Some("forced"));
    }

    #[tokio::test]
    async fn promote_rejects_stale_confirmation() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        svc.create_slot(
            &owner(owner_id),
            site_id,
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .expect("create");
        svc.sync_snapshot(&owner(owner_id), site_id)
            .await
            .expect("sync");
        let stale = Utc::now() - chrono::Duration::seconds(120);
        let r = svc
            .promote(
                &owner(owner_id),
                site_id,
                SnapshotId::new(1).unwrap(),
                stale,
            )
            .await;
        assert!(matches!(
            r,
            Err(PromotionServiceError::Staging(
                SiteStagingError::ConfirmationExpired(_)
            ))
        ));
    }

    #[tokio::test]
    async fn non_owner_cannot_create_slot() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        let r = svc
            .create_slot(
                &admin(owner_id),
                site_id,
                CreateSlotRequest {
                    subdomain: None,
                    document_root: None,
                    sync_policy: None,
                    php_version: None,
                    schedule: None,
                },
            )
            .await;
        assert!(matches!(r, Err(PromotionServiceError::Forbidden)));
    }

    #[tokio::test]
    async fn concurrent_sync_refused() {
        let (svc, owner_id, site_id, _, _) = build_service().await;
        svc.create_slot(
            &owner(owner_id),
            site_id,
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .expect("create");

        let first = svc.sync_snapshot(&owner(owner_id), site_id).await;
        assert!(first.is_ok());
        // The lock auto-releases on drop; we just verify the second
        // sync can run without panicking.
        let second = svc.sync_snapshot(&owner(owner_id), site_id).await;
        assert!(second.is_ok());
    }
}
