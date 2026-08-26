//! Per-site transport-tuning persistence and use cases. The policy
//! lives in a JSON column on the sites table; `NULL` means the
//! byte-identical default profile.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{Role, SiteError, TransportPolicy, User};
use sqlx::SqlitePool;

/// Read/write access to a site's [`TransportPolicy`].
pub struct SiteTransportService {
    pool: SqlitePool,
    audit: Arc<dyn AuditService>,
}

impl SiteTransportService {
    /// Construct over the shared pool and audit sink.
    pub fn new(pool: SqlitePool, audit: Arc<dyn AuditService>) -> Self {
        Self { pool, audit }
    }

    /// Load a site's transport policy; unset policies read back as
    /// the default profile.
    pub async fn get(
        &self,
        caller: &User,
        site_id: uuid::Uuid,
    ) -> Result<TransportPolicy, SiteError> {
        Self::require_authorized(caller)?;
        let row: Option<Option<String>> =
            sqlx::query_scalar("SELECT transport_policy FROM sites WHERE id = ?")
                .bind(site_id.to_string())
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| SiteError::Persistence(e.to_string()))?;
        let stored = row.ok_or(SiteError::NotFound(site_id.to_string()))?;
        match stored {
            Some(json) => {
                serde_json::from_str(&json).map_err(|e| SiteError::InvalidTransport(e.to_string()))
            }
            None => Ok(TransportPolicy::default()),
        }
    }

    /// Validate and persist a site's transport policy.
    pub async fn set(
        &self,
        caller: &User,
        site_id: uuid::Uuid,
        policy: TransportPolicy,
    ) -> Result<TransportPolicy, SiteError> {
        Self::require_authorized(caller)?;
        let exists: Option<String> = sqlx::query_scalar("SELECT id FROM sites WHERE id = ?")
            .bind(site_id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SiteError::Persistence(e.to_string()))?;
        if exists.is_none() {
            return Err(SiteError::NotFound(site_id.to_string()));
        }
        let json = serde_json::to_string(&policy)
            .map_err(|e| SiteError::InvalidTransport(e.to_string()))?;
        sqlx::query("UPDATE sites SET transport_policy = ? WHERE id = ?")
            .bind(&json)
            .bind(site_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SiteError::Persistence(e.to_string()))?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::SiteTransportChanged,
                    AuditOutcome::Success,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "http3_enabled": policy.http3_enabled(),
                    "tls_min_version": format!("{:?}", policy.tls_min_version()),
                    "hsts": policy.hsts().is_some(),
                    "body_size_cap_bytes": policy.body_size_cap().as_u64(),
                })),
            )
            .await;
        Ok(policy)
    }

    fn require_authorized(caller: &User) -> Result<(), SiteError> {
        if caller.role() == Role::Owner || caller.role() == Role::Admin {
            Ok(())
        } else {
            Err(SiteError::Forbidden)
        }
    }
}
