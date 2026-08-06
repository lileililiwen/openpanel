//! Audit service: append-only event log. Owned by the architecture layer;
//! every bounded context that mutates state calls `record()`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Sqlite};

use crate::error::CoreResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Failure,
    Denied,
}

impl AuditOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditOutcome::Success => "success",
            AuditOutcome::Failure => "failure",
            AuditOutcome::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    Login,
    Logout,
    UserCreated,
    UserDisabled,
    UserDeleted,
    RoleChanged,
    PasswordChanged,
    PermissionDenied,
    SiteCreated,
    SiteDeleted,
    SiteEnabled,
    SiteDisabled,
    SiteOwnerChanged,
    SslIssued,
    DatabaseCreated,
    FileUploaded,
    FileDeleted,
}

impl AuditAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Login => "login",
            AuditAction::Logout => "logout",
            AuditAction::UserCreated => "user_created",
            AuditAction::UserDisabled => "user_disabled",
            AuditAction::UserDeleted => "user_deleted",
            AuditAction::RoleChanged => "role_changed",
            AuditAction::PasswordChanged => "password_changed",
            AuditAction::PermissionDenied => "permission_denied",
            AuditAction::SiteCreated => "site_created",
            AuditAction::SiteDeleted => "site_deleted",
            AuditAction::SiteEnabled => "site_enabled",
            AuditAction::SiteDisabled => "site_disabled",
            AuditAction::SiteOwnerChanged => "site_owner_changed",
            AuditAction::SslIssued => "ssl_issued",
            AuditAction::DatabaseCreated => "database_created",
            AuditAction::FileUploaded => "file_uploaded",
            AuditAction::FileDeleted => "file_deleted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub ts: DateTime<Utc>,
    pub actor: String,
    pub action: AuditAction,
    pub target: Option<String>,
    pub source_ip: Option<String>,
    pub outcome: AuditOutcome,
    pub metadata: Value,
}

impl AuditEvent {
    pub fn new(actor: impl Into<String>, action: AuditAction, outcome: AuditOutcome) -> Self {
        Self {
            ts: Utc::now(),
            actor: actor.into(),
            action,
            target: None,
            source_ip: None,
            outcome,
            metadata: Value::Null,
        }
    }

    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    pub fn source_ip(mut self, ip: impl Into<String>) -> Self {
        self.source_ip = Some(ip.into());
        self
    }

    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = value;
        self
    }
}

#[async_trait]
pub trait AuditService: Send + Sync + 'static {
    async fn record(&self, event: AuditEvent) -> CoreResult<()>;
    async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>>;
}

/// No-op audit used in tests when the SQLite pool is not available.
pub struct NoopAuditService;

#[async_trait]
impl AuditService for NoopAuditService {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        tracing::debug!(?event, "audit");
        Ok(())
    }
    async fn recent(&self, _limit: i64) -> CoreResult<Vec<AuditEvent>> {
        Ok(Vec::new())
    }
}

pub struct SqliteAuditService {
    pool: Pool<Sqlite>,
}

impl SqliteAuditService {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    pub async fn ensure_schema(&self) -> CoreResult<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS audit_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                ts TEXT NOT NULL,
                actor TEXT NOT NULL,
                action TEXT NOT NULL,
                target TEXT,
                source_ip TEXT,
                outcome TEXT NOT NULL,
                metadata TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action)",
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[async_trait]
impl AuditService for SqliteAuditService {
    async fn record(&self, event: AuditEvent) -> CoreResult<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_log (ts, actor, action, target, source_ip, outcome, metadata)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.ts.to_rfc3339())
        .bind(&event.actor)
        .bind(event.action.as_str())
        .bind(&event.target)
        .bind(&event.source_ip)
        .bind(event.outcome.as_str())
        .bind(event.metadata.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn recent(&self, limit: i64) -> CoreResult<Vec<AuditEvent>> {
        let rows = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, String, Option<String>)>(
            "SELECT ts, actor, action, target, source_ip, outcome, metadata FROM audit_log ORDER BY ts DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let mut events = Vec::with_capacity(rows.len());
        for (ts, actor, action, target, source_ip, outcome, metadata) in rows {
            let action = match action.as_str() {
                "login" => AuditAction::Login,
                "logout" => AuditAction::Logout,
                "user_created" => AuditAction::UserCreated,
                "user_disabled" => AuditAction::UserDisabled,
                "user_deleted" => AuditAction::UserDeleted,
                "role_changed" => AuditAction::RoleChanged,
                "password_changed" => AuditAction::PasswordChanged,
                "permission_denied" => AuditAction::PermissionDenied,
                "site_created" => AuditAction::SiteCreated,
                "site_deleted" => AuditAction::SiteDeleted,
                "site_enabled" => AuditAction::SiteEnabled,
                "site_disabled" => AuditAction::SiteDisabled,
                "site_owner_changed" => AuditAction::SiteOwnerChanged,
                "ssl_issued" => AuditAction::SslIssued,
                "database_created" => AuditAction::DatabaseCreated,
                "file_uploaded" => AuditAction::FileUploaded,
                "file_deleted" => AuditAction::FileDeleted,
                other => {
                    tracing::warn!(other, "unknown audit action");
                    continue;
                }
            };
            let outcome = match outcome.as_str() {
                "success" => AuditOutcome::Success,
                "failure" => AuditOutcome::Failure,
                "denied" => AuditOutcome::Denied,
                _ => AuditOutcome::Failure,
            };
            let md = metadata
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(Value::Null);
            events.push(AuditEvent {
                ts: chrono::DateTime::parse_from_rfc3339(&ts)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                actor,
                action,
                target,
                source_ip,
                outcome,
                metadata: md,
            });
        }
        Ok(events)
    }
}

/// Convenience wrapper for `Arc<dyn AuditService>` callers.
pub type SharedAudit = Arc<dyn AuditService>;

pub fn noop() -> SharedAudit {
    Arc::new(NoopAuditService)
}