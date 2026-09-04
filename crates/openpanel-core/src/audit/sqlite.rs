//! SQLite-backed audit log: append, list, and paginated query.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{Pool, Sqlite};

use crate::error::CoreResult;

use super::{
    AuditAction, AuditEvent, AuditOutcome, AuditPage, AuditQuery, AuditService, AuditView,
    cursor::AuditCursor,
};

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

/// Parse a stored action string into its typed enumeration.
///
/// Returns `None` for unknown actions so callers can skip rows that the
/// running binary does not recognise (forwards-compatible storage).
fn parse_action(s: &str) -> Option<AuditAction> {
    Some(match s {
        "login" => AuditAction::Login,
        "logout" => AuditAction::Logout,
        "user_created" => AuditAction::UserCreated,
        "user_updated" => AuditAction::UserUpdated,
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
        "site_http_controls_changed" => AuditAction::SiteHttpControlsChanged,
        "site_transport_changed" => AuditAction::SiteTransportChanged,
        "db_remote_access_changed" => AuditAction::DbRemoteAccessChanged,
        "runtime_changed" => AuditAction::RuntimeChanged,
        "logs_policy_changed" => AuditAction::LogsPolicyChanged,
        "ssh_key_changed" => AuditAction::SshKeyChanged,
        "deliverability_checked" => AuditAction::DeliverabilityChecked,
        "terminal_opened" => AuditAction::TerminalOpened,
        "terminal_closed" => AuditAction::TerminalClosed,
        "sso_login" => AuditAction::SsoLogin,
        "snapshot_created" => AuditAction::SnapshotCreated,
        "snapshot_restored" => AuditAction::SnapshotRestored,
        "snapshot_pruned" => AuditAction::SnapshotPruned,
        "session_revoked" => AuditAction::SessionRevoked,
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
        "preview_created" => AuditAction::PreviewCreated,
        "preview_ready" => AuditAction::PreviewReady,
        "preview_destroyed" => AuditAction::PreviewDestroyed,
        "container_start_quota_blocked" => AuditAction::ContainerStartQuotaBlocked,
        "container_quota_plan_override" => AuditAction::ContainerQuotaPlanOverride,
        "container_image_pulled" => AuditAction::ContainerImagePulled,
        "container_egress_limit_raised" => AuditAction::ContainerEgressLimitRaised,
        "bandwidth_threshold_crossed" => AuditAction::BandwidthThresholdCrossed,
        "plan_created" => AuditAction::PlanCreated,
        "plan_updated" => AuditAction::PlanUpdated,
        "plan_disabled" => AuditAction::PlanDisabled,
        "plan_enabled" => AuditAction::PlanEnabled,
        "plan_cloned" => AuditAction::PlanCloned,
        "plan_deleted" => AuditAction::PlanDeleted,
        "plan_delete_blocked" => AuditAction::PlanDeleteBlocked,
        "plan_assigned" => AuditAction::PlanAssigned,
        "plan_reassigned" => AuditAction::PlanReassigned,
        "plan_unassigned" => AuditAction::PlanUnassigned,
        "account_hierarchy_child_created" => AuditAction::AccountHierarchyChildCreated,
        "account_hierarchy_attached" => AuditAction::AccountHierarchyAttached,
        "account_hierarchy_detached" => AuditAction::AccountHierarchyDetached,
        "account_hierarchy_pool_set" => AuditAction::AccountHierarchyPoolSet,
        "account_hierarchy_pool_claimed" => AuditAction::AccountHierarchyPoolClaimed,
        "account_hierarchy_pool_released" => AuditAction::AccountHierarchyPoolReleased,
        "quota_policy_changed" => AuditAction::QuotaPolicyChanged,
        "quota_policy_deleted" => AuditAction::QuotaPolicyDeleted,
        "quota_soft_limit_reached" => AuditAction::QuotaSoftLimitReached,
        "quota_hard_limit_reached" => AuditAction::QuotaHardLimitReached,
        "agent_registered" => AuditAction::AgentRegistered,
        "agent_revoked" => AuditAction::AgentRevoked,
        "fleet_token_issued" => AuditAction::FleetTokenIssued,
        "fleet_token_revoked" => AuditAction::FleetTokenRevoked,
        "recipe_manifest_stored" => AuditAction::RecipeManifestStored,
        "cluster_node_declared" => AuditAction::ClusterNodeDeclared,
        "cluster_node_role_changed" => AuditAction::ClusterNodeRoleChanged,
        "cluster_storage_declared" => AuditAction::ClusterStorageDeclared,
        "cluster_replicated_database_declared" => AuditAction::ClusterReplicatedDatabaseDeclared,
        "migration_previewed" => AuditAction::MigrationPreviewed,
        "migration_run_committed" => AuditAction::MigrationRunCommitted,
        "migration_run_rolled_back" => AuditAction::MigrationRunRolledBack,
        "migration_rollback_completed" => AuditAction::MigrationRollbackCompleted,
        "migration_already_imported_rejected" => AuditAction::MigrationAlreadyImportedRejected,
        "backup_credential_created" => AuditAction::BackupCredentialCreated,
        "backup_credential_deleted" => AuditAction::BackupCredentialDeleted,
        "backup_credential_in_use_rejected" => AuditAction::BackupCredentialInUseRejected,
        "backup_remote_target_attached" => AuditAction::BackupRemoteTargetAttached,
        "backup_remote_tested" => AuditAction::BackupRemoteTested,
        "site_cache_policy_updated" => AuditAction::SiteCachePolicyUpdated,
        "site_cache_policy_applied" => AuditAction::SiteCachePolicyApplied,
        "site_cache_policy_rolled_back" => AuditAction::SiteCachePolicyRolledBack,
        "cdn_integration_created" => AuditAction::CdnIntegrationCreated,
        "cdn_integration_deleted" => AuditAction::CdnIntegrationDeleted,
        "cdn_purged" => AuditAction::CdnPurged,
        "cdn_purge_partial" => AuditAction::CdnPurgePartial,
        "cdn_credential_decrypt_failed" => AuditAction::CdnCredentialDecryptFailed,
        "site_cloned" => AuditAction::SiteCloned,
        "site_cloned_from_template" => AuditAction::SiteClonedFromTemplate,
        "clone_pii_anonymised" => AuditAction::ClonePiiAnonymised,
        "clone_kept_pii" => AuditAction::CloneKeptPii,
        "site_template_exported" => AuditAction::SiteTemplateExported,
        "template_signature_failed" => AuditAction::TemplateSignatureFailed,
        "theme_override_updated" => AuditAction::ThemeOverrideUpdated,
        "theme_override_cleared" => AuditAction::ThemeOverrideCleared,
        "web_app_install_planned" => AuditAction::WebAppInstallPlanned,
        "web_app_installed" => AuditAction::WebAppInstalled,
        "install_artifact_rejected" => AuditAction::InstallArtifactRejected,
        "web_app_uninstalled" => AuditAction::WebAppUninstalled,
        "web_app_uninstalled_dropped_db" => AuditAction::WebAppUninstalledDroppedDb,
        "scan_profile_created" => AuditAction::ScanProfileCreated,
        "scan_completed" => AuditAction::ScanCompleted,
        "quarantine_record_created" => AuditAction::QuarantineRecordCreated,
        "quarantine_record_restored" => AuditAction::QuarantineRecordRestored,
        "site_blocked_quarantined" => AuditAction::SiteBlockedQuarantined,
        "site_block_expired" => AuditAction::SiteBlockExpired,
        "webmail_session_created" => AuditAction::WebmailSessionCreated,
        "webmail_csrf_rejected" => AuditAction::WebmailCsrfRejected,
        _ => return None,
    })
}

/// Parse a stored outcome string into its typed enumeration.
fn parse_outcome(s: &str) -> AuditOutcome {
    match s {
        "success" => AuditOutcome::Success,
        "failure" => AuditOutcome::Failure,
        "denied" => AuditOutcome::Denied,
        _ => AuditOutcome::Failure,
    }
}

/// Reconstruct a typed [`AuditEvent`] from raw stored columns, returning
/// `None` (and skipping the row) when the action is unrecognised.
#[allow(clippy::too_many_arguments)]
fn parse_audit_row(
    id: Option<i64>,
    ts: String,
    actor: String,
    action: String,
    target: Option<String>,
    source_ip: Option<String>,
    outcome: String,
    metadata: Option<String>,
) -> Option<(AuditEvent, Option<i64>)> {
    let action = parse_action(&action)?;
    let outcome = parse_outcome(&outcome);
    let md = metadata
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let ts = DateTime::parse_from_rfc3339(&ts)
        .map(|d| d.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    Some((
        AuditEvent {
            ts,
            actor,
            action,
            target,
            source_ip,
            outcome,
            metadata: md,
        },
        id,
    ))
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
                "user_updated" => AuditAction::UserUpdated,
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
                "site_http_controls_changed" => AuditAction::SiteHttpControlsChanged,
                "site_transport_changed" => AuditAction::SiteTransportChanged,
                "db_remote_access_changed" => AuditAction::DbRemoteAccessChanged,
                "runtime_changed" => AuditAction::RuntimeChanged,
                "logs_policy_changed" => AuditAction::LogsPolicyChanged,
                "ssh_key_changed" => AuditAction::SshKeyChanged,
                "deliverability_checked" => AuditAction::DeliverabilityChecked,
                "terminal_opened" => AuditAction::TerminalOpened,
                "terminal_closed" => AuditAction::TerminalClosed,
                "sso_login" => AuditAction::SsoLogin,
                "snapshot_created" => AuditAction::SnapshotCreated,
                "snapshot_restored" => AuditAction::SnapshotRestored,
                "snapshot_pruned" => AuditAction::SnapshotPruned,
                "session_revoked" => AuditAction::SessionRevoked,
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
                "preview_created" => AuditAction::PreviewCreated,
                "preview_ready" => AuditAction::PreviewReady,
                "preview_destroyed" => AuditAction::PreviewDestroyed,
                "container_start_quota_blocked" => AuditAction::ContainerStartQuotaBlocked,
                "container_quota_plan_override" => AuditAction::ContainerQuotaPlanOverride,
                "container_image_pulled" => AuditAction::ContainerImagePulled,
                "container_egress_limit_raised" => AuditAction::ContainerEgressLimitRaised,
                "bandwidth_threshold_crossed" => AuditAction::BandwidthThresholdCrossed,
                "plan_created" => AuditAction::PlanCreated,
                "plan_updated" => AuditAction::PlanUpdated,
                "plan_disabled" => AuditAction::PlanDisabled,
                "plan_enabled" => AuditAction::PlanEnabled,
                "plan_cloned" => AuditAction::PlanCloned,
                "plan_deleted" => AuditAction::PlanDeleted,
                "plan_delete_blocked" => AuditAction::PlanDeleteBlocked,
                "plan_assigned" => AuditAction::PlanAssigned,
                "plan_reassigned" => AuditAction::PlanReassigned,
                "plan_unassigned" => AuditAction::PlanUnassigned,
                "account_hierarchy_child_created" => AuditAction::AccountHierarchyChildCreated,
                "account_hierarchy_attached" => AuditAction::AccountHierarchyAttached,
                "account_hierarchy_detached" => AuditAction::AccountHierarchyDetached,
                "account_hierarchy_pool_set" => AuditAction::AccountHierarchyPoolSet,
                "account_hierarchy_pool_claimed" => AuditAction::AccountHierarchyPoolClaimed,
                "account_hierarchy_pool_released" => AuditAction::AccountHierarchyPoolReleased,
                "quota_policy_changed" => AuditAction::QuotaPolicyChanged,
                "quota_policy_deleted" => AuditAction::QuotaPolicyDeleted,
                "quota_soft_limit_reached" => AuditAction::QuotaSoftLimitReached,
                "quota_hard_limit_reached" => AuditAction::QuotaHardLimitReached,
                "agent_registered" => AuditAction::AgentRegistered,
                "agent_revoked" => AuditAction::AgentRevoked,
                "fleet_token_issued" => AuditAction::FleetTokenIssued,
                "fleet_token_revoked" => AuditAction::FleetTokenRevoked,
                "recipe_manifest_stored" => AuditAction::RecipeManifestStored,
                "cluster_node_declared" => AuditAction::ClusterNodeDeclared,
                "cluster_node_role_changed" => AuditAction::ClusterNodeRoleChanged,
                "cluster_storage_declared" => AuditAction::ClusterStorageDeclared,
                "cluster_replicated_database_declared" => {
                    AuditAction::ClusterReplicatedDatabaseDeclared
                }
                "migration_previewed" => AuditAction::MigrationPreviewed,
                "migration_run_committed" => AuditAction::MigrationRunCommitted,
                "migration_run_rolled_back" => AuditAction::MigrationRunRolledBack,
                "migration_rollback_completed" => AuditAction::MigrationRollbackCompleted,
                "migration_already_imported_rejected" => {
                    AuditAction::MigrationAlreadyImportedRejected
                }
                "backup_credential_created" => AuditAction::BackupCredentialCreated,
                "backup_credential_deleted" => AuditAction::BackupCredentialDeleted,
                "backup_credential_in_use_rejected" => AuditAction::BackupCredentialInUseRejected,
                "backup_remote_target_attached" => AuditAction::BackupRemoteTargetAttached,
                "backup_remote_tested" => AuditAction::BackupRemoteTested,
                "site_cache_policy_updated" => AuditAction::SiteCachePolicyUpdated,
                "site_cache_policy_applied" => AuditAction::SiteCachePolicyApplied,
                "site_cache_policy_rolled_back" => AuditAction::SiteCachePolicyRolledBack,
                "cdn_integration_created" => AuditAction::CdnIntegrationCreated,
                "cdn_integration_deleted" => AuditAction::CdnIntegrationDeleted,
                "cdn_purged" => AuditAction::CdnPurged,
                "cdn_purge_partial" => AuditAction::CdnPurgePartial,
                "cdn_credential_decrypt_failed" => AuditAction::CdnCredentialDecryptFailed,
                "site_cloned" => AuditAction::SiteCloned,
                "site_cloned_from_template" => AuditAction::SiteClonedFromTemplate,
                "clone_pii_anonymised" => AuditAction::ClonePiiAnonymised,
                "clone_kept_pii" => AuditAction::CloneKeptPii,
                "site_template_exported" => AuditAction::SiteTemplateExported,
                "template_signature_failed" => AuditAction::TemplateSignatureFailed,
                "theme_override_updated" => AuditAction::ThemeOverrideUpdated,
                "theme_override_cleared" => AuditAction::ThemeOverrideCleared,
                "web_app_install_planned" => AuditAction::WebAppInstallPlanned,
                "web_app_installed" => AuditAction::WebAppInstalled,
                "install_artifact_rejected" => AuditAction::InstallArtifactRejected,
                "web_app_uninstalled" => AuditAction::WebAppUninstalled,
                "web_app_uninstalled_dropped_db" => AuditAction::WebAppUninstalledDroppedDb,
                "scan_profile_created" => AuditAction::ScanProfileCreated,
                "scan_completed" => AuditAction::ScanCompleted,
                "quarantine_record_created" => AuditAction::QuarantineRecordCreated,
                "quarantine_record_restored" => AuditAction::QuarantineRecordRestored,
                "site_blocked_quarantined" => AuditAction::SiteBlockedQuarantined,
                "site_block_expired" => AuditAction::SiteBlockExpired,
                "webmail_session_created" => AuditAction::WebmailSessionCreated,
                "webmail_csrf_rejected" => AuditAction::WebmailCsrfRejected,
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

    async fn query(&self, q: AuditQuery) -> CoreResult<AuditPage> {
        // Fetch one extra row to detect whether a next page exists.
        let fetch = q.effective_limit();
        let mut sql = String::from(
            "SELECT id, ts, actor, action, target, source_ip, outcome, metadata FROM audit_log",
        );
        let mut conds: Vec<&str> = Vec::new();
        if q.actor.is_some() {
            conds.push("actor = ?");
        }
        if q.action.is_some() {
            conds.push("action = ?");
        }
        if q.target.is_some() {
            conds.push("target LIKE ? ESCAPE '\\'");
        }
        if q.outcome.is_some() {
            conds.push("outcome = ?");
        }
        if q.from.is_some() {
            conds.push("ts >= ?");
        }
        if q.to.is_some() {
            conds.push("ts <= ?");
        }
        if q.cursor.is_some() {
            conds.push("(ts < ? OR (ts = ? AND id < ?))");
        }
        if !conds.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conds.join(" AND "));
        }
        sql.push_str(" ORDER BY ts DESC, id DESC LIMIT ?");

        let mut query = sqlx::query_as::<
            _,
            (
                i64,
                String,
                String,
                String,
                Option<String>,
                Option<String>,
                String,
                Option<String>,
            ),
        >(&sql);
        if let Some(v) = &q.actor {
            query = query.bind(v);
        }
        if let Some(v) = &q.action {
            query = query.bind(v);
        }
        if let Some(v) = &q.target {
            query = query.bind(format!("%{}%", v.replace('\\', "\\\\")));
        }
        if let Some(v) = &q.outcome {
            query = query.bind(v);
        }
        if let Some(v) = &q.from {
            query = query.bind(v.to_rfc3339());
        }
        if let Some(v) = &q.to {
            query = query.bind(v.to_rfc3339());
        }
        if let Some(c) = &q.cursor {
            query = query.bind(c.ts.to_rfc3339());
            query = query.bind(c.ts.to_rfc3339());
            query = query.bind(c.id);
        }
        query = query.bind(fetch);

        let rows = query.fetch_all(&self.pool).await?;
        let has_more = rows.len() as i64 == fetch;
        let (rows, extra) = if has_more {
            let n = rows.len();
            (rows[..n - 1].to_vec(), Some(rows[n - 1].clone()))
        } else {
            (rows.clone(), rows.last().cloned())
        };

        let mut events = Vec::with_capacity(rows.len());
        for (id, ts, actor, action, target, source_ip, outcome, metadata) in rows {
            if let Some((ev, _)) = parse_audit_row(
                Some(id),
                ts,
                actor,
                action,
                target,
                source_ip,
                outcome,
                metadata,
            ) {
                events.push(AuditView::from_event(&ev));
            }
        }

        let next_cursor = if has_more {
            extra.and_then(|(id, ts, _, _, _, _, _, _)| {
                let parsed = DateTime::parse_from_rfc3339(&ts).ok()?.with_timezone(&Utc);
                Some(AuditCursor { ts: parsed, id })
            })
        } else {
            None
        };

        Ok(AuditPage {
            events,
            next_cursor,
        })
    }
}
