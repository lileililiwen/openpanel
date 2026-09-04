//! Audited action taxonomy and its persisted-name serialisation.

use serde::{Deserialize, Serialize};

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
    /// A user account was updated (parent_account_id or
    /// hosting_plan_id round-tripped through the identity service).
    /// The metadata field records the changed field and the new
    /// value (or `None` for a clear).
    UserUpdated,
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
    /// DNSSEC was enabled for a zone.
    DnsSecEnabled,
    /// DNSSEC was disabled for a zone.
    DnsSecDisabled,
    /// A DS record was published to the parent zone.
    DnsSecDsPublished,
    /// An anti-spam policy was applied.
    AntispamChanged,
    /// A Sieve filter was compiled and saved.
    SieveApplied,
    /// A git deploy was triggered for a site.
    GitDeployed,
    /// A maintenance window was created.
    MaintenanceWindowCreated,
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
    /// A per-site HTTP-controls document changed.
    SiteHttpControlsChanged,
    /// A per-site transport policy changed.
    SiteTransportChanged,
    /// A log rotation policy changed or a manual rotation ran.
    LogsPolicyChanged,
    /// The admin authorized_keys file was rewritten.
    SshKeyChanged,
    /// A deliverability blocklist check completed.
    DeliverabilityChecked,
    /// A browser terminal session was opened.
    TerminalOpened,
    /// A browser terminal session was closed, with the reason.
    TerminalClosed,
    /// A panel login completed through the external IdP.
    SsoLogin,
    /// A whole-server snapshot was created.
    SnapshotCreated,
    /// A whole-server snapshot was restored.
    SnapshotRestored,
    /// A snapshot beyond retention was pruned.
    SnapshotPruned,
    /// A session was revoked by its owner or an admin.
    SessionRevoked,
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
    /// A preview deployment was created.
    PreviewCreated,
    /// A preview deployment became ready.
    PreviewReady,
    /// A preview deployment was destroyed.
    PreviewDestroyed,
    /// A status page policy mutation (enable/disable, slug rotation,
    /// publish/unpublish, label edit).
    StatusPagePolicyChanged,
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
    /// A container start was blocked because it would exceed a
    /// per-user quota axis. Metadata records the axis.
    ContainerStartQuotaBlocked,
    /// A plan cap tightened a user-set container quota axis; the
    /// audit metadata records the tightened value.
    ContainerQuotaPlanOverride,
    /// An image was pulled for a container using a registry
    /// credential; metadata records `{owner_id, container_id,
    /// image_ref, ref_count}` only — never the manifest body
    /// or any credential secret.
    ContainerImagePulled,
    /// An owner raised a container's monthly egress limit; the
    /// throttle that was applied at 80% is removed.
    ContainerEgressLimitRaised,
    /// A container crossed 80% of its monthly egress quota; the
    /// container is throttled (1 Mbps) until the owner raises the
    /// limit. Consumed by the bandwidth accounting bounded context.
    BandwidthThresholdCrossed,
    /// A hosting plan was created.
    PlanCreated,
    /// A hosting plan was updated (caps, features, or allowed apps).
    PlanUpdated,
    /// A hosting plan was disabled.
    PlanDisabled,
    /// A previously disabled plan was re-enabled.
    PlanEnabled,
    /// A hosting plan was cloned from an existing plan.
    PlanCloned,
    /// A hosting plan was deleted.
    PlanDeleted,
    /// A delete request was refused because the plan still has
    /// current assignments.
    PlanDeleteBlocked,
    /// A user was assigned to a hosting plan.
    PlanAssigned,
    /// A user was reassigned from one plan to another.
    PlanReassigned,
    /// A hosting plan was detached from a user.
    PlanUnassigned,
    /// A child account was created by a parent.
    AccountHierarchyChildCreated,
    /// A child account was attached to a parent.
    AccountHierarchyAttached,
    /// A child account was detached from a parent.
    AccountHierarchyDetached,
    /// A parent declared or updated a quota pool.
    AccountHierarchyPoolSet,
    /// A child claimed share bytes from a parent's pool.
    AccountHierarchyPoolClaimed,
    /// A child released share bytes from a parent's pool.
    AccountHierarchyPoolReleased,
    /// A quota policy was created or updated.
    QuotaPolicyChanged,
    /// A quota policy was deleted.
    QuotaPolicyDeleted,
    /// A user exceeded a soft quota limit (warning).
    QuotaSoftLimitReached,
    /// A user exceeded a hard quota limit (blocking).
    QuotaHardLimitReached,
    /// An agent was registered with the control plane.
    AgentRegistered,
    /// An agent was revoked by the control plane.
    AgentRevoked,
    /// A fleet token was issued to an agent.
    FleetTokenIssued,
    /// A fleet token was revoked.
    FleetTokenRevoked,
    /// A signed recipe manifest was stored for later dispatch.
    RecipeManifestStored,
    /// A cluster node was declared.
    ClusterNodeDeclared,
    /// A cluster node's role was changed.
    ClusterNodeRoleChanged,
    /// A shared storage volume was declared.
    ClusterStorageDeclared,
    /// A replicated database was declared.
    ClusterReplicatedDatabaseDeclared,
    /// A migration source bundle was previewed (dry run).
    MigrationPreviewed,
    /// A confirmed migration plan committed its imported resources.
    MigrationRunCommitted,
    /// A migration run failed and was rolled back to its commit point.
    MigrationRunRolledBack,
    /// A migration rollback undo ran and deleted the imported resources.
    MigrationRollbackCompleted,
    /// A source bundle was refused because it was already imported.
    MigrationAlreadyImportedRejected,
    /// An offsite backup credential was created (encrypted at rest).
    BackupCredentialCreated,
    /// An offsite backup credential was deleted.
    BackupCredentialDeleted,
    /// A deletion was refused because the credential is in use.
    BackupCredentialInUseRejected,
    /// A remote target was attached to a backup plan.
    BackupRemoteTargetAttached,
    /// A remote target reachability probe completed.
    BackupRemoteTested,
    /// A site's cache policy was updated.
    SiteCachePolicyUpdated,
    /// A site's cache policy was applied to nginx (snippet written, reload issued).
    SiteCachePolicyApplied,
    /// A site's cache policy was rolled back because `nginx -t` failed.
    SiteCachePolicyRolledBack,
    /// A CDN integration was created.
    CdnIntegrationCreated,
    /// A CDN integration was deleted.
    CdnIntegrationDeleted,
    /// A CDN purge completed.
    CdnPurged,
    /// A CDN purge partially failed; the adapter rejected one or more paths.
    CdnPurgePartial,
    /// Decryption of a stored CDN credential blob failed (tamper or wrong key).
    CdnCredentialDecryptFailed,
    /// A site clone completed.
    SiteCloned,
    /// A site was cloned from a stored template.
    SiteClonedFromTemplate,
    /// PII was anonymised during a clone.
    ClonePiiAnonymised,
    /// A clone explicitly kept PII (Owner opt-in).
    CloneKeptPii,
    /// A site was exported as a reusable template.
    SiteTemplateExported,
    /// A template signature failed to verify on retrieval.
    TemplateSignatureFailed,
    /// A theme override was updated.
    ThemeOverrideUpdated,
    /// A theme override was cleared.
    ThemeOverrideCleared,
    /// A web-app install plan was created.
    WebAppInstallPlanned,
    /// A web-app install completed.
    WebAppInstalled,
    /// An install artifact was rejected (sha256 mismatch).
    InstallArtifactRejected,
    /// A web-app was uninstalled (files archived).
    WebAppUninstalled,
    /// A web-app was uninstalled and its database dropped.
    WebAppUninstalledDroppedDb,
    /// A scan profile was created.
    ScanProfileCreated,
    /// A scan completed.
    ScanCompleted,
    /// A file was moved to quarantine.
    QuarantineRecordCreated,
    /// A quarantined file was restored.
    QuarantineRecordRestored,
    /// A site was blocked due to a quarantine policy.
    SiteBlockedQuarantined,
    /// A site block expired.
    SiteBlockExpired,
    /// A webmail session was created.
    WebmailSessionCreated,
    /// A webmail CSRF rejection occurred.
    WebmailCsrfRejected,
}

impl AuditAction {
    /// Serialized form used when persisting to the audit log.
    pub fn as_str(&self) -> &'static str {
        match self {
            AuditAction::Login => "login",
            AuditAction::Logout => "logout",
            AuditAction::UserCreated => "user_created",
            AuditAction::UserUpdated => "user_updated",
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
            AuditAction::PreviewCreated => "preview_created",
            AuditAction::PreviewReady => "preview_ready",
            AuditAction::PreviewDestroyed => "preview_destroyed",
            AuditAction::StatusPagePolicyChanged => "status_page_policy_changed",
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
            AuditAction::DnsSecEnabled => "dnssec_enabled",
            AuditAction::DnsSecDisabled => "dnssec_disabled",
            AuditAction::DnsSecDsPublished => "dnssec_ds_published",
            AuditAction::AntispamChanged => "antispam_changed",
            AuditAction::SieveApplied => "sieve_applied",
            AuditAction::GitDeployed => "git_deployed",
            AuditAction::MaintenanceWindowCreated => "maintenance_window_created",
            AuditAction::OsUpdateApplied => "os_update_applied",
            AuditAction::ContainerStartQuotaBlocked => "container_start_quota_blocked",
            AuditAction::ContainerQuotaPlanOverride => "container_quota_plan_override",
            AuditAction::ContainerImagePulled => "container_image_pulled",
            AuditAction::ContainerEgressLimitRaised => "container_egress_limit_raised",
            AuditAction::BandwidthThresholdCrossed => "bandwidth_threshold_crossed",
            AuditAction::PlanCreated => "plan_created",
            AuditAction::PlanUpdated => "plan_updated",
            AuditAction::PlanDisabled => "plan_disabled",
            AuditAction::PlanEnabled => "plan_enabled",
            AuditAction::PlanCloned => "plan_cloned",
            AuditAction::PlanDeleted => "plan_deleted",
            AuditAction::PlanDeleteBlocked => "plan_delete_blocked",
            AuditAction::PlanAssigned => "plan_assigned",
            AuditAction::PlanReassigned => "plan_reassigned",
            AuditAction::PlanUnassigned => "plan_unassigned",
            AuditAction::AccountHierarchyChildCreated => "account_hierarchy_child_created",
            AuditAction::AccountHierarchyAttached => "account_hierarchy_attached",
            AuditAction::AccountHierarchyDetached => "account_hierarchy_detached",
            AuditAction::AccountHierarchyPoolSet => "account_hierarchy_pool_set",
            AuditAction::AccountHierarchyPoolClaimed => "account_hierarchy_pool_claimed",
            AuditAction::AccountHierarchyPoolReleased => "account_hierarchy_pool_released",
            AuditAction::QuotaPolicyChanged => "quota_policy_changed",
            AuditAction::QuotaPolicyDeleted => "quota_policy_deleted",
            AuditAction::QuotaSoftLimitReached => "quota_soft_limit_reached",
            AuditAction::QuotaHardLimitReached => "quota_hard_limit_reached",
            AuditAction::AgentRegistered => "agent_registered",
            AuditAction::AgentRevoked => "agent_revoked",
            AuditAction::FleetTokenIssued => "fleet_token_issued",
            AuditAction::FleetTokenRevoked => "fleet_token_revoked",
            AuditAction::RecipeManifestStored => "recipe_manifest_stored",
            AuditAction::ClusterNodeDeclared => "cluster_node_declared",
            AuditAction::ClusterNodeRoleChanged => "cluster_node_role_changed",
            AuditAction::ClusterStorageDeclared => "cluster_storage_declared",
            AuditAction::ClusterReplicatedDatabaseDeclared => {
                "cluster_replicated_database_declared"
            }
            AuditAction::MigrationPreviewed => "migration_previewed",
            AuditAction::MigrationRunCommitted => "migration_run_committed",
            AuditAction::MigrationRunRolledBack => "migration_run_rolled_back",
            AuditAction::MigrationRollbackCompleted => "migration_rollback_completed",
            AuditAction::MigrationAlreadyImportedRejected => "migration_already_imported_rejected",
            AuditAction::BackupCredentialCreated => "backup_credential_created",
            AuditAction::BackupCredentialDeleted => "backup_credential_deleted",
            AuditAction::BackupCredentialInUseRejected => "backup_credential_in_use_rejected",
            AuditAction::BackupRemoteTargetAttached => "backup_remote_target_attached",
            AuditAction::BackupRemoteTested => "backup_remote_tested",
            AuditAction::SiteCachePolicyUpdated => "site_cache_policy_updated",
            AuditAction::SiteCachePolicyApplied => "site_cache_policy_applied",
            AuditAction::SiteCachePolicyRolledBack => "site_cache_policy_rolled_back",
            AuditAction::CdnIntegrationCreated => "cdn_integration_created",
            AuditAction::CdnIntegrationDeleted => "cdn_integration_deleted",
            AuditAction::CdnPurged => "cdn_purged",
            AuditAction::CdnPurgePartial => "cdn_purge_partial",
            AuditAction::CdnCredentialDecryptFailed => "cdn_credential_decrypt_failed",
            AuditAction::SiteCloned => "site_cloned",
            AuditAction::SiteClonedFromTemplate => "site_cloned_from_template",
            AuditAction::ClonePiiAnonymised => "clone_pii_anonymised",
            AuditAction::CloneKeptPii => "clone_kept_pii",
            AuditAction::SiteTemplateExported => "site_template_exported",
            AuditAction::TemplateSignatureFailed => "template_signature_failed",
            AuditAction::ThemeOverrideUpdated => "theme_override_updated",
            AuditAction::ThemeOverrideCleared => "theme_override_cleared",
            AuditAction::WebAppInstallPlanned => "web_app_install_planned",
            AuditAction::WebAppInstalled => "web_app_installed",
            AuditAction::InstallArtifactRejected => "install_artifact_rejected",
            AuditAction::WebAppUninstalled => "web_app_uninstalled",
            AuditAction::WebAppUninstalledDroppedDb => "web_app_uninstalled_dropped_db",
            AuditAction::ScanProfileCreated => "scan_profile_created",
            AuditAction::ScanCompleted => "scan_completed",
            AuditAction::QuarantineRecordCreated => "quarantine_record_created",
            AuditAction::QuarantineRecordRestored => "quarantine_record_restored",
            AuditAction::SiteBlockedQuarantined => "site_blocked_quarantined",
            AuditAction::SiteBlockExpired => "site_block_expired",
            AuditAction::WebmailSessionCreated => "webmail_session_created",
            AuditAction::WebmailCsrfRejected => "webmail_csrf_rejected",
            AuditAction::SiteHttpControlsChanged => "site_http_controls_changed",
            AuditAction::SiteTransportChanged => "site_transport_changed",
            AuditAction::LogsPolicyChanged => "logs_policy_changed",
            AuditAction::SshKeyChanged => "ssh_key_changed",
            AuditAction::DeliverabilityChecked => "deliverability_checked",
            AuditAction::TerminalOpened => "terminal_opened",
            AuditAction::TerminalClosed => "terminal_closed",
            AuditAction::SsoLogin => "sso_login",
            AuditAction::SnapshotCreated => "snapshot_created",
            AuditAction::SnapshotRestored => "snapshot_restored",
            AuditAction::SnapshotPruned => "snapshot_pruned",
            AuditAction::SessionRevoked => "session_revoked",
        }
    }
}
