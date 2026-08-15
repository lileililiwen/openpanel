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
    /// A database privilege was granted.
    DbPrivilegeGranted,
    /// A database privilege was revoked.
    DbPrivilegeRevoked,
    /// Remote access was toggled for a managed database.
    DbRemoteAccessChanged,
    /// A short-lived SSO session was issued for the admin tool.
    DbAdminToolLaunched,
    /// An IP address pool was created or updated.
    IpPoolChanged,
    /// An IP address was allocated to a site.
    IpAllocated,
    /// An IP address was released back to its pool.
    IpReleased,
    /// A site's vhost was rebound to its current address set.
    IpVhostRebound,
    /// A chargeback was computed for an owner.
    BillingChargebackComputed,
    /// A signed billing webhook was accepted.
    BillingWebhookAccepted,
    /// A billing webhook was rejected (bad signature, disabled, …).
    BillingWebhookRejected,
    /// A load-balancer member's status was updated.
    LbMemberChanged,
    /// A WordPress security scan completed.
    WpScanned,
    /// A WordPress update applied cleanly.
    WpUpdated,
    /// A WordPress update failed and was rolled back.
    WpUpdateRolledBack,
    /// A WordPress cache mode was changed.
    WpCacheChanged,
    /// A wildcard cert was issued successfully.
    WildcardCertIssued,
    /// A wildcard cert attempt failed.
    WildcardCertFailed,
    /// A site's runtime was created, updated, or started.
    RuntimeChanged,
    /// An isolation policy was applied successfully.
    IsolationApplied,
    /// An isolation policy enforce failed (isolation retained).
    IsolationEnforceFailed,
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
    /// A Web entry's pinned artifact was downloaded and placed on
    /// disk. The audit metadata records the source URL, the digest
    /// (or `digest_verified: false` for a documented placeholder),
    /// the bytes written, and the host platform.
    SoftwareArtifactInstalled,
    /// A user enrolled a TOTP or WebAuthn second factor.
    TwoFactorEnrolled,
    /// A second-factor verification succeeded during login.
    TwoFactorVerified,
    /// A second-factor verification failed during login.
    TwoFactorFailed,
    /// A second factor was revoked.
    TwoFactorRevoked,
    /// A single-use recovery code was consumed during login.
    RecoveryCodeConsumed,
    /// A valid remember-device cookie skipped the second-factor
    /// challenge for a subsequent login.
    DeviceRemembered,
    /// A per-site web application firewall policy changed.
    WafChanged,
    /// A trusted container image was pulled.
    DockerImagePulled,
    /// An unsafe container specification was rejected.
    DockerSpecRejected,
    /// A forbidden container capability was denied.
    DockerCapabilityDenied,
    /// A container was OOM-killed.
    DockerOomKilled,
    /// A container lifecycle operation completed.
    DockerChanged,
    /// A site network denied access to the control plane.
    DockerEgressDenied,
    /// An FTP account or its enabled state changed.
    FtpChanged,
    /// An FTP login succeeded.
    FtpLogin,
    /// An FTP login was denied without exposing credentials.
    FtpLoginDenied,
    /// An FTP path attempted to escape its site root.
    FtpChrootEscape,
    /// An FTP client attempted plaintext authentication when TLS is required.
    FtpTlsRequired,
    /// An FTP connection ceiling rejected a session.
    FtpConcurrentLimit,
    /// An FTP session exhausted its transfer-byte allowance.
    FtpTransferLimit,
    /// The supervised FTP listener could not bind.
    FtpBindFailed,
    /// The supervised FTP listener restarted after a panic.
    FtpListenerRestart,
    /// A personal API token was created.
    TokenCreated,
    /// A personal API token was rotated.
    TokenRotated,
    /// A personal API token was revoked.
    TokenRevoked,
    /// A bearer-authenticated request succeeded.
    TokenRequest,
    /// An expired bearer was rejected.
    TokenExpired,
    /// A bearer lacked the declared route scope.
    TokenScopeRejected,
    /// A bearer source address was outside its allowlist.
    TokenCidrRejected,
    /// A bearer exhausted its per-token bucket.
    TokenRateLimited,
    /// A notification channel or subscription changed.
    NotificationChanged,
    /// A notification destination was rejected by policy.
    DeliveryRejected,
    /// A notification adapter accepted a delivery.
    DeliverySucceeded,
    /// A notification delivery exhausted retries or failed permanently.
    DeliveryFailed,
    /// A database point-in-time recovery binlog stream was enabled.
    PitrStreamEnabled,
    /// A database point-in-time recovery binlog stream was paused.
    PitrStreamPaused,
    /// A database point-in-time recovery binlog stream was resumed.
    PitrStreamResumed,
    /// A database point-in-time recovery binlog stream hit an error.
    PitrStreamBroken,
    /// A database point-in-time restore was requested.
    PitrRestoreRequested,
    /// A staging point-in-time restore was promoted to live.
    PitrRestorePromoted,
    /// A point-in-time restore failed.
    PitrRestoreFailed,
    /// An incremental database backup delta was captured.
    PitrIncrementalCaptured,
    /// A per-site staging slot was created.
    StagingSlotCreated,
    /// A per-site staging slot was deleted.
    StagingSlotDeleted,
    /// A staging snapshot was taken.
    StagingSnapshotTaken,
    /// A staging snapshot was promoted to live.
    StagingPromoted,
    /// A staging promotion was rolled back.
    StagingPromotionRolledBack,
    /// A plugin manifest was installed.
    PluginInstalled,
    /// An installed plugin was enabled.
    PluginEnabled,
    /// An installed plugin was disabled.
    PluginDisabled,
    /// An installed plugin was uninstalled.
    PluginUninstalled,
    /// A plugin manifest was rejected during signature verification.
    PluginManifestRejected,
    /// A plugin was installed from the marketplace catalog.
    PluginInstalledFromMarketplace,
    /// A per-site collaborator was invited.
    CollaboratorInvited,
    /// A per-site collaborator's permissions were updated.
    CollaboratorUpdated,
    /// A per-site collaborator was revoked from a site.
    CollaboratorRevoked,
    /// A container image was pushed to the registry.
    RegistryImagePushed,
    /// A container image was pruned by the retention policy.
    RegistryImagePruned,
    /// A user posted a question to the AI Ops agent.
    AiAsked,
    /// The AI Ops agent invoked an allowlisted tool.
    AiToolCalled,
    /// An AI Ops write action was approved and executed.
    AiActionExecuted,
    /// An AI Ops write action was denied by a human approver.
    AiActionDenied,
    /// A CIS hardening profile was applied.
    HardeningApplied,
    /// A previously applied CIS rule was rolled back.
    HardeningReverted,
    /// The audit retention policy was changed.
    AuditRetentionChanged,
    /// Audit records older than the retention cutoff were purged.
    AuditPurged,
    /// A GDPR export was generated for a user.
    GdprExportRequested,
    /// An OS update was applied (security or other).
    OsUpdateApplied,
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
            AuditAction::SoftwareArtifactInstalled => "software_artifact_installed",
            AuditAction::TwoFactorEnrolled => "two_factor_enrolled",
            AuditAction::TwoFactorVerified => "two_factor_verified",
            AuditAction::TwoFactorFailed => "two_factor_failed",
            AuditAction::TwoFactorRevoked => "two_factor_revoked",
            AuditAction::RecoveryCodeConsumed => "recovery_code_consumed",
            AuditAction::DeviceRemembered => "device_remembered",
            AuditAction::WafChanged => "waf_changed",
            AuditAction::DockerImagePulled => "docker_image_pulled",
            AuditAction::DockerSpecRejected => "docker_spec_rejected",
            AuditAction::DockerCapabilityDenied => "docker_capability_denied",
            AuditAction::DockerOomKilled => "docker_oom_killed",
            AuditAction::DockerChanged => "docker_changed",
            AuditAction::DockerEgressDenied => "docker_egress_denied",
            AuditAction::FtpChanged => "ftp_changed",
            AuditAction::FtpLogin => "ftp_login",
            AuditAction::FtpLoginDenied => "ftp_login_denied",
            AuditAction::FtpChrootEscape => "ftp_chroot_escape",
            AuditAction::FtpTlsRequired => "ftp_tls_required",
            AuditAction::FtpConcurrentLimit => "ftp_concurrent_limit",
            AuditAction::FtpTransferLimit => "ftp_transfer_limit",
            AuditAction::FtpBindFailed => "ftp_bind_failed",
            AuditAction::FtpListenerRestart => "ftp_listener_restart",
            AuditAction::TokenCreated => "token_created",
            AuditAction::TokenRotated => "token_rotated",
            AuditAction::TokenRevoked => "token_revoked",
            AuditAction::TokenRequest => "token_request",
            AuditAction::TokenExpired => "token_expired",
            AuditAction::TokenScopeRejected => "token_scope_rejected",
            AuditAction::TokenCidrRejected => "token_cidr_rejected",
            AuditAction::TokenRateLimited => "token_rate_limited",
            AuditAction::NotificationChanged => "notification_changed",
            AuditAction::DeliveryRejected => "delivery_rejected",
            AuditAction::DeliverySucceeded => "delivery_succeeded",
            AuditAction::DeliveryFailed => "delivery_failed",
            AuditAction::PitrStreamEnabled => "pitr_stream_enabled",
            AuditAction::PitrStreamPaused => "pitr_stream_paused",
            AuditAction::PitrStreamResumed => "pitr_stream_resumed",
            AuditAction::PitrStreamBroken => "pitr_stream_broken",
            AuditAction::PitrRestoreRequested => "pitr_restore_requested",
            AuditAction::PitrRestorePromoted => "pitr_restore_promoted",
            AuditAction::PitrRestoreFailed => "pitr_restore_failed",
            AuditAction::PitrIncrementalCaptured => "pitr_incremental_captured",
            AuditAction::StagingSlotCreated => "staging_slot_created",
            AuditAction::StagingSlotDeleted => "staging_slot_deleted",
            AuditAction::StagingSnapshotTaken => "staging_snapshot_taken",
            AuditAction::StagingPromoted => "staging_promoted",
            AuditAction::StagingPromotionRolledBack => "staging_promotion_rolled_back",
            AuditAction::PluginInstalled => "plugin_installed",
            AuditAction::PluginEnabled => "plugin_enabled",
            AuditAction::PluginDisabled => "plugin_disabled",
            AuditAction::PluginUninstalled => "plugin_uninstalled",
            AuditAction::PluginManifestRejected => "plugin_manifest_rejected",
            AuditAction::PluginInstalledFromMarketplace => "plugin_installed_from_marketplace",
            AuditAction::CollaboratorInvited => "collaborator_invited",
            AuditAction::CollaboratorUpdated => "collaborator_updated",
            AuditAction::CollaboratorRevoked => "collaborator_revoked",
            AuditAction::RegistryImagePushed => "registry_image_pushed",
            AuditAction::RegistryImagePruned => "registry_image_pruned",
            AuditAction::AiAsked => "ai_asked",
            AuditAction::AiToolCalled => "ai_tool_called",
            AuditAction::AiActionExecuted => "ai_action_executed",
            AuditAction::AiActionDenied => "ai_action_denied",
            AuditAction::HardeningApplied => "hardening_applied",
            AuditAction::HardeningReverted => "hardening_reverted",
            AuditAction::AuditRetentionChanged => "audit_retention_changed",
            AuditAction::AuditPurged => "audit_purged",
            AuditAction::GdprExportRequested => "gdpr_export_requested",
            AuditAction::DbPrivilegeGranted => "db_privilege_granted",
            AuditAction::DbPrivilegeRevoked => "db_privilege_revoked",
            AuditAction::DbRemoteAccessChanged => "db_remote_access_changed",
            AuditAction::DbAdminToolLaunched => "db_admin_tool_launched",
            AuditAction::IpPoolChanged => "ip_pool_changed",
            AuditAction::IpAllocated => "ip_allocated",
            AuditAction::IpReleased => "ip_released",
            AuditAction::IpVhostRebound => "ip_vhost_rebound",
            AuditAction::BillingChargebackComputed => "billing_chargeback_computed",
            AuditAction::BillingWebhookAccepted => "billing_webhook_accepted",
            AuditAction::BillingWebhookRejected => "billing_webhook_rejected",
            AuditAction::LbMemberChanged => "lb_member_changed",
            AuditAction::WpScanned => "wp_scanned",
            AuditAction::WpUpdated => "wp_updated",
            AuditAction::WpUpdateRolledBack => "wp_update_rolled_back",
            AuditAction::WpCacheChanged => "wp_cache_changed",
            AuditAction::WildcardCertIssued => "wildcard_cert_issued",
            AuditAction::WildcardCertFailed => "wildcard_cert_failed",
            AuditAction::RuntimeChanged => "runtime_changed",
            AuditAction::IsolationApplied => "isolation_applied",
            AuditAction::IsolationEnforceFailed => "isolation_enforce_failed",
            AuditAction::OsUpdateApplied => "os_update_applied",
            AuditAction::LogDownloaded => "log_downloaded",
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
                "dns_changed" => AuditAction::DnsChanged,
                "mail_changed" => AuditAction::MailChanged,
                "software_changed" => AuditAction::SoftwareChanged,
                "software_artifact_installed" => AuditAction::SoftwareArtifactInstalled,
                "two_factor_enrolled" => AuditAction::TwoFactorEnrolled,
                "two_factor_verified" => AuditAction::TwoFactorVerified,
                "two_factor_failed" => AuditAction::TwoFactorFailed,
                "two_factor_revoked" => AuditAction::TwoFactorRevoked,
                "recovery_code_consumed" => AuditAction::RecoveryCodeConsumed,
                "device_remembered" => AuditAction::DeviceRemembered,
                "waf_changed" => AuditAction::WafChanged,
                "docker_image_pulled" => AuditAction::DockerImagePulled,
                "docker_spec_rejected" => AuditAction::DockerSpecRejected,
                "docker_capability_denied" => AuditAction::DockerCapabilityDenied,
                "docker_oom_killed" => AuditAction::DockerOomKilled,
                "docker_changed" => AuditAction::DockerChanged,
                "docker_egress_denied" => AuditAction::DockerEgressDenied,
                "ftp_changed" => AuditAction::FtpChanged,
                "ftp_login" => AuditAction::FtpLogin,
                "ftp_login_denied" => AuditAction::FtpLoginDenied,
                "ftp_chroot_escape" => AuditAction::FtpChrootEscape,
                "ftp_tls_required" => AuditAction::FtpTlsRequired,
                "ftp_concurrent_limit" => AuditAction::FtpConcurrentLimit,
                "ftp_transfer_limit" => AuditAction::FtpTransferLimit,
                "ftp_bind_failed" => AuditAction::FtpBindFailed,
                "ftp_listener_restart" => AuditAction::FtpListenerRestart,
                "token_created" => AuditAction::TokenCreated,
                "token_rotated" => AuditAction::TokenRotated,
                "token_revoked" => AuditAction::TokenRevoked,
                "token_request" => AuditAction::TokenRequest,
                "token_expired" => AuditAction::TokenExpired,
                "token_scope_rejected" => AuditAction::TokenScopeRejected,
                "token_cidr_rejected" => AuditAction::TokenCidrRejected,
                "token_rate_limited" => AuditAction::TokenRateLimited,
                "notification_changed" => AuditAction::NotificationChanged,
                "delivery_rejected" => AuditAction::DeliveryRejected,
                "delivery_succeeded" => AuditAction::DeliverySucceeded,
                "delivery_failed" => AuditAction::DeliveryFailed,
                "pitr_stream_enabled" => AuditAction::PitrStreamEnabled,
                "pitr_stream_paused" => AuditAction::PitrStreamPaused,
                "pitr_stream_resumed" => AuditAction::PitrStreamResumed,
                "pitr_stream_broken" => AuditAction::PitrStreamBroken,
                "pitr_restore_requested" => AuditAction::PitrRestoreRequested,
                "pitr_restore_promoted" => AuditAction::PitrRestorePromoted,
                "pitr_restore_failed" => AuditAction::PitrRestoreFailed,
                "pitr_incremental_captured" => AuditAction::PitrIncrementalCaptured,
                "staging_slot_created" => AuditAction::StagingSlotCreated,
                "staging_slot_deleted" => AuditAction::StagingSlotDeleted,
                "staging_snapshot_taken" => AuditAction::StagingSnapshotTaken,
                "staging_promoted" => AuditAction::StagingPromoted,
                "staging_promotion_rolled_back" => AuditAction::StagingPromotionRolledBack,
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
