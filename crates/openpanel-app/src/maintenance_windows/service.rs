//! Scheduled maintenance windows services: enforcer.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    DestructiveActionClass, MaintenanceError, MaintenanceOverride, MaintenanceRepository,
    MaintenanceWindow, Role, User,
};
use uuid::Uuid;

use crate::maintenance_windows::SqliteMaintenanceRepository;

/// Maintenance enforcer. The enforcer refuses destructive
/// actions when a maintenance window is active unless the
/// caller presents a valid single-use override.
pub struct MaintenanceEnforcer {
    repo: Arc<SqliteMaintenanceRepository>,
    audit: Arc<dyn AuditService>,
}

impl MaintenanceEnforcer {
    /// Construct an enforcer.
    pub fn new(repo: Arc<SqliteMaintenanceRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Schedule a new maintenance window.
    pub async fn schedule(
        &self,
        caller: &User,
        window: MaintenanceWindow,
    ) -> Result<MaintenanceWindow, MaintenanceError> {
        require_admin(caller)?;
        window.validate()?;
        // Reject overlap with an existing window.
        let existing = self.repo.list_windows().await?;
        for w in &existing {
            if !(window.ends_at <= w.starts_at || window.starts_at >= w.ends_at) {
                return Err(MaintenanceError::WindowOverlap);
            }
        }
        self.repo.save_window(&window).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::MaintenanceWindowCreated,
                    AuditOutcome::Success,
                )
                .target(window.id.to_string())
                .metadata(serde_json::json!({
                    "label": window.label,
                })),
            )
            .await;
        Ok(window)
    }

    /// Delete a maintenance window.
    pub async fn cancel(&self, caller: &User, id: Uuid) -> Result<(), MaintenanceError> {
        require_admin(caller)?;
        self.repo.delete_window(id).await?;
        Ok(())
    }

    /// Issue a single-use override. The caller MUST be Admin/Owner.
    pub async fn issue_override(
        &self,
        caller: &User,
        target: DestructiveActionClass,
        reason: &str,
        ttl_secs: u32,
    ) -> Result<MaintenanceOverride, MaintenanceError> {
        require_admin(caller)?;
        let now = Utc::now();
        let override_ = MaintenanceOverride {
            id: Uuid::new_v4(),
            target_class: target,
            reason: reason.to_string(),
            ttl_secs,
            created_at: now,
            expires_at: now + Duration::seconds(ttl_secs as i64),
            consumed_at: None,
            issued_by: caller.id(),
        };
        self.repo.save_override(&override_).await?;
        Ok(override_)
    }

    /// Gate a destructive action. Returns `Ok(())` when the
    /// caller is allowed to proceed. Refuses when an active
    /// maintenance window covers the action class and no valid
    /// override was presented.
    pub async fn gate(
        &self,
        caller: &User,
        action: DestructiveActionClass,
        override_id: Option<Uuid>,
    ) -> Result<(), MaintenanceError> {
        require_admin(caller)?;
        let windows = self.repo.list_windows().await?;
        let now = Utc::now();
        let active: Vec<&MaintenanceWindow> = windows
            .iter()
            .filter(|w| w.is_active(now) && w.blocked_classes.contains(&action))
            .collect();
        if active.is_empty() {
            return Ok(());
        }
        let Some(override_id) = override_id else {
            return Err(MaintenanceError::DestructiveBlocked);
        };
        let mut override_ = self
            .repo
            .get_override(override_id)
            .await?
            .ok_or(MaintenanceError::OverrideMissing)?;
        if !override_.is_valid(now) {
            return Err(MaintenanceError::OverrideExpired);
        }
        if override_.target_class != action {
            return Err(MaintenanceError::OverrideMissing);
        }
        override_.consume(now);
        self.repo.save_override(&override_).await?;
        Ok(())
    }
}

fn require_admin(caller: &User) -> Result<(), MaintenanceError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(MaintenanceError::Forbidden),
    }
}
