//! Audit service: append-only event log. Owned by the architecture layer;
//! every bounded context that mutates state calls `record()`.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Pool, Sqlite};

use crate::error::CoreResult;

/// Action-name serialisation tables (kept out of `mod.rs` to stay
/// under the per-file line budget).
mod strings;

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
    /// Query the audit log with typed filters and cursor pagination.
    ///
    /// Returns a page of redacted, render-safe event views ordered
    /// newest-first with a deterministic next cursor.
    async fn query(&self, query: AuditQuery) -> CoreResult<AuditPage>;
}

/// Opaque, URL-safe pagination cursor encoding the last returned row's
/// `(ts, id)` so the next page continues deterministically, even when
/// many events share a timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditCursor {
    /// Timestamp of the boundary row.
    pub ts: DateTime<Utc>,
    /// Primary-key id of the boundary row.
    pub id: i64,
}

impl AuditCursor {
    /// Encode the cursor into a stable, non-sensitive string.
    pub fn encode(&self) -> String {
        encode_base64(format!("{}|{}", self.ts.to_rfc3339(), self.id).as_bytes())
    }

    /// Decode a cursor previously produced by [`AuditCursor::encode`].
    pub fn decode(s: &str) -> Option<Self> {
        let raw = decode_base64(s).ok()?;
        let text = String::from_utf8(raw).ok()?;
        let (ts_part, id_part) = text.split_once('|')?;
        let ts = DateTime::parse_from_rfc3339(ts_part)
            .ok()?
            .with_timezone(&Utc);
        let id = id_part.parse::<i64>().ok()?;
        Some(Self { ts, id })
    }
}

/// Filters applied to the audit log query.
#[derive(Debug, Clone, Default)]
pub struct AuditQuery {
    /// Exact actor (user or service) identifier.
    pub actor: Option<String>,
    /// Exact action value (stored snake_case string, e.g. `site_created`).
    pub action: Option<String>,
    /// Substring match on the target the action was performed on.
    pub target: Option<String>,
    /// Exact outcome value (`success` / `failure` / `denied`).
    pub outcome: Option<String>,
    /// Inclusive lower bound on event timestamp.
    pub from: Option<DateTime<Utc>>,
    /// Inclusive upper bound on event timestamp.
    pub to: Option<DateTime<Utc>>,
    /// Cursor returned by a previous page; continues after it.
    pub cursor: Option<AuditCursor>,
    /// Maximum number of events to return (clamped to 1..=200).
    pub limit: usize,
}

impl AuditQuery {
    /// A fresh query returning the default page size.
    pub fn new() -> Self {
        Self {
            limit: 50,
            ..Default::default()
        }
    }

    /// Attach a decoded cursor, ignoring an unparseable value.
    pub fn with_cursor(mut self, cursor: Option<&str>) -> Self {
        self.cursor = cursor.and_then(AuditCursor::decode);
        self
    }

    /// Clamp `limit` into the supported `1..=200` range.
    pub fn effective_limit(&self) -> i64 {
        (self.limit.clamp(1, 200) as i64) + 1
    }
}

/// A page of audit events plus the cursor for the next page (if any).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditPage {
    /// Render-safe event views for this page.
    pub events: Vec<AuditView>,
    /// Cursor for the next page, or `None` when this is the last page.
    pub next_cursor: Option<AuditCursor>,
}

/// A render-safe projection of an [`AuditEvent`].
///
/// The `metadata` field has already passed through the central
/// redaction allowlist, so it is safe to serialize to HTML or JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditView {
    /// When the event occurred.
    pub ts: DateTime<Utc>,
    /// The actor (user or service) that performed the action.
    pub actor: String,
    /// The action that was performed (snake_case string).
    pub action: String,
    /// Optional target the action was performed on.
    pub target: Option<String>,
    /// Optional source IP address of the request.
    pub source_ip: Option<String>,
    /// Whether the action succeeded, failed, or was denied.
    pub outcome: String,
    /// Redacted, render-safe metadata attached to the event.
    pub metadata: Value,
}

impl AuditView {
    /// Build a redacted, render-safe projection of an [`AuditEvent`].
    pub fn from_event(event: &AuditEvent) -> Self {
        Self {
            ts: event.ts,
            actor: event.actor.clone(),
            action: event.action.as_str().to_string(),
            target: event.target.clone(),
            source_ip: event.source_ip.clone(),
            outcome: event.outcome.as_str().to_string(),
            metadata: redact_metadata(&event.metadata),
        }
    }
}

/// Metadata keys that are never rendered, regardless of value.
const SECRET_KEYS: &[&str] = &[
    "password",
    "passwd",
    "pwd",
    "token",
    "secret",
    "key",
    "api_key",
    "apikey",
    "private",
    "pem",
    "cert",
    "certificate",
    "csr",
    "credential",
    "credentials",
    "body",
    "passphrase",
    "auth",
    "authorization",
    "session",
    "cookie",
    "bearer",
    "otp",
    "signature",
    "salt",
    "webhook",
    "hook",
    "ssh",
    "private_key",
];

/// Whether a metadata key names a secret and must be dropped.
fn is_secret_key(key: &str) -> bool {
    let k = key.to_lowercase();
    SECRET_KEYS.iter().any(|s| k.contains(s))
}

/// Whether a string value looks like a secret (PEM, JWT, or a long
/// high-entropy blob) and should be replaced.
fn looks_like_secret(value: &str) -> bool {
    let v = value.trim();
    if v.starts_with("-----BEGIN") {
        return true;
    }
    if v.starts_with("eyJ") && v.split('.').count() == 3 {
        return true;
    }
    let all_alnum = v
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'));
    if v.len() >= 40 && all_alnum {
        return true;
    }
    let all_hex = !v.is_empty() && v.chars().all(|c| c.is_ascii_hexdigit());
    if v.len() >= 64 && all_hex {
        return true;
    }
    false
}

/// Recursively redact secret-shaped keys and values from an audit
/// metadata document. Secret keys are dropped; secret-looking string
/// values are replaced with a redaction marker; everything else is kept.
pub fn redact_metadata(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                if is_secret_key(k) {
                    continue;
                }
                out.insert(k.clone(), redact_metadata(v));
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(redact_metadata).collect()),
        Value::String(s) => {
            if looks_like_secret(s) {
                Value::String("***redacted***".to_string())
            } else {
                Value::String(s.clone())
            }
        }
        other => other.clone(),
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Minimal standard-base64 encoder (no external dependency).
fn encode_base64(input: &[u8]) -> String {
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(B64[((n >> 18) & 63) as usize] as char);
        out.push(B64[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Minimal standard-base64 decoder (no external dependency).
fn decode_base64(input: &str) -> Result<Vec<u8>, ()> {
    let mut groups: Vec<u8> = Vec::new();
    for c in input.trim().chars() {
        if c == '=' {
            continue;
        }
        let v = B64.iter().position(|&x| x == c as u8).ok_or(())?;
        groups.push(v as u8);
    }
    let mut out = Vec::new();
    for chunk in groups.chunks(4) {
        if chunk.len() < 2 {
            break;
        }
        let n = ((chunk[0] as u32) << 18)
            | ((chunk.get(1).copied().unwrap_or(0) as u32) << 12)
            | ((chunk.get(2).copied().unwrap_or(0) as u32) << 6)
            | (chunk.get(3).copied().unwrap_or(0) as u32);
        out.push((n >> 16) as u8);
        if chunk.len() > 2 {
            out.push((n >> 8) as u8);
        }
        if chunk.len() > 3 {
            out.push(n as u8);
        }
    }
    Ok(out)
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

    async fn query(&self, _query: AuditQuery) -> CoreResult<AuditPage> {
        Ok(AuditPage {
            events: Vec::new(),
            next_cursor: None,
        })
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

/// Convenience wrapper for `Arc<dyn AuditService>` callers.
pub type SharedAudit = Arc<dyn AuditService>;

/// Build a shared no-op audit service.
pub fn noop() -> SharedAudit {
    Arc::new(NoopAuditService)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn sample(action: AuditAction, outcome: AuditOutcome, meta: Value) -> AuditEvent {
        AuditEvent::new("admin", action, outcome)
            .target("site.example")
            .metadata(meta)
    }

    #[test]
    fn redaction_drops_secret_keys() {
        let md = serde_json::json!({
            "password": "hunter2",
            "token": "abc.def.ghi",
            "target_id": "1234",
            "note": "hello",
        });
        let view =
            AuditView::from_event(&sample(AuditAction::UserCreated, AuditOutcome::Success, md));
        assert!(!view.metadata.as_object().unwrap().contains_key("password"));
        assert!(!view.metadata.as_object().unwrap().contains_key("token"));
        assert_eq!(view.metadata["target_id"], "1234");
        assert_eq!(view.metadata["note"], "hello");
    }

    #[test]
    fn redaction_scrubs_secret_values() {
        let md = serde_json::json!({
            "note": "normal text",
            "body": "-----BEGIN PRIVATE KEY-----\nMIIE...\n-----END PRIVATE KEY-----",
            "jwt": "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        });
        let view = AuditView::from_event(&sample(
            AuditAction::SettingsChanged,
            AuditOutcome::Success,
            md,
        ));
        // Secret-shaped keys are dropped entirely.
        assert!(!view.metadata.as_object().unwrap().contains_key("body"));
        // Non-secret keys holding secret-shaped values are redacted in place.
        assert_eq!(view.metadata["note"], "normal text");
        assert_eq!(view.metadata["jwt"], "***redacted***");
    }

    #[test]
    fn redaction_recurses_into_nested_objects() {
        let md = serde_json::json!({
            "outer": { "api_key": "secret", "ok": "kept" },
        });
        let view =
            AuditView::from_event(&sample(AuditAction::SslIssued, AuditOutcome::Success, md));
        let outer = view.metadata["outer"].as_object().unwrap();
        assert!(!outer.contains_key("api_key"));
        assert_eq!(outer["ok"], "kept");
    }

    #[test]
    fn cursor_round_trips() {
        let c = AuditCursor {
            ts: Utc::now(),
            id: 42,
        };
        let encoded = c.encode();
        let decoded = AuditCursor::decode(&encoded).expect("decode");
        assert_eq!(decoded, c);
    }

    #[test]
    fn cursor_is_stable_across_round_trip_with_millis() {
        let c = AuditCursor {
            ts: DateTime::parse_from_rfc3339("2026-08-29T12:34:56.789Z")
                .unwrap()
                .with_timezone(&Utc),
            id: 7,
        };
        assert_eq!(AuditCursor::decode(&c.encode()), Some(c));
    }

    #[test]
    fn query_default_limit_is_positive() {
        let q = AuditQuery::new();
        assert!(q.effective_limit() > 1);
    }

    #[test]
    fn query_with_cursor_ignores_garbage() {
        let q = AuditQuery::new().with_cursor(Some("not-a-cursor"));
        assert!(q.cursor.is_none());
    }
}
