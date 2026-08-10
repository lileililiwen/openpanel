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
/// Result of an audited operation.
pub enum AuditOutcome {
    /// The operation completed successfully.
    Success,
    /// The operation failed.
    Failure,
    /// The operation was denied (e.g. missing permission).
    Denied,
}

impl AuditOutcome {
    /// Serialized form used when persisting to the audit log.
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
/// An audited action performed on the system.
pub enum AuditAction {
    /// User successfully logged in.
    Login,
    /// User logged out.
    Logout,
    /// A user account was created.
    UserCreated,
    /// A user account was disabled.
    UserDisabled,
    /// A user account was re-enabled.
    UserEnabled,
    /// A user account was deleted.
    UserDeleted,
    /// A user's role was changed.
    RoleChanged,
    /// A user's password was changed.
    PasswordChanged,
    /// An operation was denied due to permissions.
    PermissionDenied,
    /// A website/site was created.
    SiteCreated,
    /// A website/site was deleted.
    SiteDeleted,
    /// A website/site was enabled.
    SiteEnabled,
    /// A website/site was disabled.
    SiteDisabled,
    /// A website/site's owner was changed.
    SiteOwnerChanged,
    /// An SSL certificate was issued.
    SslIssued,
    /// An SSL certificate was manually uploaded.
    SslManualUploaded,
    /// A self-signed certificate was generated.
    SslSelfSignedGenerated,
    /// An SSL certificate was revoked.
    SslRevoked,
    /// An SSL certificate was deleted.
    SslDeleted,
    /// The force-HTTPS redirect for a site was toggled.
    SslForceHttpsChanged,
    /// A database was created.
    DatabaseCreated,
    /// A database was deleted.
    DatabaseDeleted,
    /// A database password was changed.
    DatabasePasswordChanged,
    /// A file was uploaded.
    FileUploaded,
    /// A file's contents were updated.
    FileUpdated,
    /// A file was renamed.
    FileRenamed,
    /// A file's mode/permissions were changed.
    FileModeChanged,
    /// A file was deleted.
    FileDeleted,
    /// A monitoring alert threshold was crossed.
    AlertFired,
    /// Allowlisted panel preferences were changed.
    SettingsChanged,
    /// A scheduled job was created, changed, enabled, disabled, or deleted.
    CronChanged,
    /// A scheduled job was manually executed.
    CronRun,
    /// A backup plan changed.
    BackupChanged,
    /// A backup run completed or failed.
    BackupRun,
    /// A restore was requested.
    BackupRestore,
    /// An authorized bounded log export was downloaded.
    LogDownloaded,
    /// OpenPanel firewall rules were applied or rolled back.
    FirewallChanged,
    /// A login-abuse block was created or ended.
    SecurityBlockChanged,
    /// A registered host service lifecycle action completed.
    ServiceChanged,
    /// A DNS provider account, zone, or record changed.
    DnsChanged,
    /// A hosted mail domain, mailbox, alias, or credential changed.
    MailChanged,
    /// A Software Center plan or installation job changed host state.
    SoftwareChanged,
}

impl AuditAction {
    /// Serialized form used when persisting to the audit log.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Login => "login",
            AuditAction::Logout => "logout",
            AuditAction::UserCreated => "user_created",
            AuditAction::UserDisabled => "user_disabled",
            AuditAction::UserEnabled => "user_enabled",
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
            AuditAction::SslManualUploaded => "ssl_manual_uploaded",
            AuditAction::SslSelfSignedGenerated => "ssl_self_signed_generated",
            AuditAction::SslRevoked => "ssl_revoked",
            AuditAction::SslDeleted => "ssl_deleted",
            AuditAction::SslForceHttpsChanged => "ssl_force_https_changed",
            AuditAction::DatabaseCreated => "database_created",
            AuditAction::DatabaseDeleted => "database_deleted",
            AuditAction::DatabasePasswordChanged => "database_password_changed",
            AuditAction::FileUploaded => "file_uploaded",
            AuditAction::FileUpdated => "file_updated",
            AuditAction::FileRenamed => "file_renamed",
            AuditAction::FileModeChanged => "file_mode_changed",
            AuditAction::FileDeleted => "file_deleted",
            AuditAction::AlertFired => "alert_fired",
            AuditAction::SettingsChanged => "settings_changed",
            AuditAction::CronChanged => "cron_changed",
            AuditAction::CronRun => "cron_run",
            AuditAction::BackupChanged => "backup_changed",
            AuditAction::BackupRun => "backup_run",
            AuditAction::BackupRestore => "backup_restore",
            AuditAction::LogDownloaded => "log_downloaded",
            AuditAction::FirewallChanged => "firewall_changed",
            AuditAction::SecurityBlockChanged => "security_block_changed",
            AuditAction::ServiceChanged => "service_changed",
            AuditAction::DnsChanged => "dns_changed",
            AuditAction::MailChanged => "mail_changed",
            AuditAction::SoftwareChanged => "software_changed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A single audited event recorded to the audit log.
pub struct AuditEvent {
    /// When the event occurred.
    pub ts: DateTime<Utc>,
    /// The actor (user or service) that performed the action.
    pub actor: String,
    /// The action that was performed.
    pub action: AuditAction,
    /// Optional target the action was performed on.
    pub target: Option<String>,
    /// Optional source IP address of the request.
    pub source_ip: Option<String>,
    /// Whether the action succeeded, failed, or was denied.
    pub outcome: AuditOutcome,
    /// Free-form JSON metadata attached to the event.
    pub metadata: Value,
}

impl AuditEvent {
    /// Create a new event with the current timestamp and no target/IP/metadata.
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

    /// Set the target the action was performed on (builder style).
    pub fn target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Set the source IP of the request (builder style).
    pub fn source_ip(mut self, ip: impl Into<String>) -> Self {
        self.source_ip = Some(ip.into());
        self
    }

    /// Set the JSON metadata attached to the event (builder style).
    pub fn metadata(mut self, value: Value) -> Self {
        self.metadata = value;
        self
    }
}

#[async_trait]
/// Contract for persisting and reading audit events.
pub trait AuditService: Send + Sync + 'static {
    /// Append an event to the audit log.
    async fn record(&self, event: AuditEvent) -> CoreResult<()>;
    /// Return the most recent events, newest first, up to `limit`.
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

/// SQLite-backed audit service persisting events to the `audit_log` table.
pub struct SqliteAuditService {
    pool: Pool<Sqlite>,
}

impl SqliteAuditService {
    /// Create a service using the given SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }

    /// Create the `audit_log` table and its indexes if they do not exist.
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
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_actor ON audit_log(actor)")
            .execute(&self.pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_audit_action ON audit_log(action)")
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
                "user_enabled" => AuditAction::UserEnabled,
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
                "ssl_manual_uploaded" => AuditAction::SslManualUploaded,
                "ssl_self_signed_generated" => AuditAction::SslSelfSignedGenerated,
                "ssl_revoked" => AuditAction::SslRevoked,
                "ssl_deleted" => AuditAction::SslDeleted,
                "ssl_force_https_changed" => AuditAction::SslForceHttpsChanged,
                "database_created" => AuditAction::DatabaseCreated,
                "database_deleted" => AuditAction::DatabaseDeleted,
                "database_password_changed" => AuditAction::DatabasePasswordChanged,
                "file_uploaded" => AuditAction::FileUploaded,
                "file_updated" => AuditAction::FileUpdated,
                "file_renamed" => AuditAction::FileRenamed,
                "file_mode_changed" => AuditAction::FileModeChanged,
                "file_deleted" => AuditAction::FileDeleted,
                "alert_fired" => AuditAction::AlertFired,
                "settings_changed" => AuditAction::SettingsChanged,
                "cron_changed" => AuditAction::CronChanged,
                "cron_run" => AuditAction::CronRun,
                "backup_changed" => AuditAction::BackupChanged,
                "backup_run" => AuditAction::BackupRun,
                "backup_restore" => AuditAction::BackupRestore,
                "log_downloaded" => AuditAction::LogDownloaded,
                "firewall_changed" => AuditAction::FirewallChanged,
                "security_block_changed" => AuditAction::SecurityBlockChanged,
                "service_changed" => AuditAction::ServiceChanged,
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

/// Build a shared no-op audit service.
pub fn noop() -> SharedAudit {
    Arc::new(NoopAuditService)
}
