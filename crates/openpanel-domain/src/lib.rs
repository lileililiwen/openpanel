//! Domain layer: entities, value objects, aggregates, repository traits.
//! Zero I/O — no sqlx, no axum, no tokio. Implementation lives in
//! `openpanel-app`.

#![deny(rustdoc::broken_intra_doc_links)]
// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production. Test code (`#[cfg(test)]`) MAY contain these, so we relax
// the deny to a warn for test compilation only.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Backup plans, manifests, runs, and safe restore values.
/// Scoped personal API token credentials and lifecycle invariants.
pub mod api_tokens;
pub mod backups;
/// Per-site collaborator bounded context: invite collaborators
/// scoped to specific sites with limited permission sets.
pub mod collaborators;
pub mod common;
/// Container image registry bounded context: per-user namespaces,
/// retention, scan hook, OCI Distribution push/pull.
pub mod container_registry;
/// Container runtime bounded context: per-user quota, registry
/// credentials, metrics, and monthly egress accounting. The docker
/// service consumes the quota gate before every container create.
pub mod container_runtime;
/// Scheduled command and HTTP job domain model.
pub mod cron;
pub mod databases;
/// Provider-backed DNS zones and typed records.
pub mod dns;
pub mod docker;
pub mod files;
/// Per-site FTP accounts, limits, and chroot path policy.
pub mod ftp;
/// Infrastructure-as-Code bounded context: SDK and Terraform
/// provider contract descriptors, regenerated from the OpenAPI
/// spec.
pub mod iac;
pub mod identity;
pub mod logs;
/// Hosted mail domains, addresses, quotas, aliases, and relay policy.
pub mod mail;
/// Web application malware scanner bounded context:
/// `ScanProfile`, `ScanRun`, `ScanFinding`, `QuarantineRecord`.
pub mod malware_scanner;
pub mod monitoring;
/// Notification channels, subscriptions, events, and durable delivery state.
pub mod notifications;
/// Offsite backup targets bounded context: the `BackupTargetAdapter`
/// contract, encrypted `BackupCredential` records, per-plan
/// `RemoteTargetConfig`, and `KekRef` wrapper records.
pub mod offsite_backup_targets;
/// Plugin extension framework: signed manifests, capability gating,
/// lifecycle, and supervisor-facing repository trait.
pub mod plugin;
/// Plugin marketplace: remote catalog discovery, publisher CA
/// verification, and rating/metadata cache.
pub mod plugin_marketplace;
pub mod security;
/// Site cache and CDN integration bounded context: the per-site
/// `SiteCachePolicy`, `CdnIntegration` aggregate, the `CdnAdapter`
/// contract, and purge / cache-level operations.
pub mod site_cache_cdn;
/// Site clone + template export bounded context: `SiteTemplate`,
/// `TemplateArtifact`, `ClonePlan`, `CloneRun`, `PiiPolicy`, and
/// `AnonymisationToken` records.
pub mod site_clone_template;
pub mod sites;
/// Trusted software catalog, transaction plans, and job lifecycle invariants.
pub mod software_center;
pub mod ssl;
/// Allowlisted host service lifecycle and health domain.
pub mod system_services;
/// Themeable UI and white-label bounded context: per-account
/// `ThemeOverride`, `Palette` with WCAG-AA contrast enforcement,
/// `Typography`, `PanelDomain`, and `BrandingScope`.
pub mod themeable_ui;
/// Per-site typed web application firewall rules.
pub mod waf;
/// Web application installer bounded context: `InstallPlan`,
/// `InstallRun`, `InstalledWebApp`, `IdempotencyKey`, and
/// `InstallArtifact` records.
pub mod web_application_installer;
/// Webmail client bounded context: short-lived session tokens and
/// the `MailBridge` contract.
pub mod webmail_client;

/// Account hierarchy bounded context: parent-child user
/// relationships, tree traversal, cycle detection, and
/// pooled quota caps shared by children.
pub mod account_hierarchy;
/// Admin IP allowlist bounded context: `AdminIpAllowlist`,
/// `AllowlistMode`, `IpCidr`, and the per-role `AllowlistOverride`
/// policy that the `IpAllowlistMiddleware` consumes.
pub mod admin_ip_allowlist;
/// Agent bounded context: per-host agent runtime, registration,
/// `FleetToken` (mTLS client cert + scoped bearer fallback), and
/// signed `RecipeManifest`. The agent refuses non-mTLS traffic and
/// validates every recipe against its allowlist before executing.
pub mod agent;
/// AI Ops bounded context: conversational session, tool-call
/// allowlist, proposed/approved/executed/denied actions.
pub mod ai_ops;
/// Non-PHP runtime bounded context: per-site runtime choice
/// (Node / Python / Go / Ruby / .NET), pinned version, app port,
/// supervisor unit, and the nginx reverse-proxy block that
/// targets `127.0.0.1:APP_PORT`.
pub mod app_runtimes;
/// Bandwidth accounting bounded context: typed bridge between
/// the `BandwidthObserver` collector and the SQLite-backed
/// rolling-window store.
pub mod bandwidth_accounting;
/// Reseller billing integration bounded context: usage meters,
/// chargeback pricing, integration state, and webhook HMAC
/// verification.
pub mod billing;
/// Cluster data model bounded context: typed host roles, shared
/// storage declarations, and replicated database metadata.
pub mod cluster_data_model;
/// Compliance bounded context: CIS hardening, audit retention,
/// GDPR export with secret redaction.
pub mod compliance;
/// Database point-in-time recovery: continuous binlog streaming,
/// point-in-time restore, and incremental file-backup deltas.
pub mod db_pitr;
/// Database privilege management bounded context: per-user grant
/// scopes, remote access with an explicit wildcard opt-in, and
/// short-lived single-use SSO tokens for the admin tool launcher.
pub mod db_privileges;
/// DNSSEC + secondary DNS bounded context: zone signing keys,
/// secondary nameserver ACLs, glue records, and DS records.
pub mod dnssec_secondary;
/// Git deployment bounded context: a per-site git repo, deploy
/// runs, and webhook HMAC verification.
pub mod git_deployment;
/// Hosting plans bounded context: plan definitions, quota caps, and
/// the `HostingPlanId` reference used by the `User` aggregate. The
/// full plan domain ships in the follow-on `add-hosting-plans` change.
pub mod hosting;
/// Hosting plans full bounded context: `HostingPlan` aggregate,
/// `PlanResolver`, feature map, and assignment table.
pub mod hosting_plans;
/// IPv6 + address-pool bounded context: typed pools, allocations
/// to sites, and the vhost binder that attaches the address set
/// to a vhost.
pub mod ip_allocation;
/// Kernel resource isolation bounded context: per-user cgroup
/// limits and namespace configuration.
pub mod kernel_isolation;
/// Load balancing and failover bounded context: pools of
/// members with health probes and weighted rotation.
pub mod load_balancing;
/// Log viewer bounded context: typed queries over a JSONL store
/// with role-based authorization.
pub mod log_viewer;
/// Mail anti-spam and filtering bounded context: per-mailbox
/// anti-spam policy, greylist, Sieve filter scripts, autoresponder
/// windows, forwarders, catch-all, and mailing lists.
pub mod mail_filtering;
/// Scheduled maintenance windows bounded context: a panel-wide
/// schedule that blocks destructive actions, with a
/// single-use override that lifts the lock for a bounded TTL.
pub mod maintenance_windows;
/// Migration importers bounded context: the `MigrationDriver`
/// contract, `MigrationPlan` preview results, `ImportedResource`
/// outcomes, and the redacted `TranslationLog`.
pub mod migration_importers;
/// OS update management bounded context: package updates,
/// unattended-upgrades policy, reboot state.
pub mod os_updates;
/// Per-site PHP runtime bounded context: `PhpRuntimeRef`,
/// `PhpFpmPoolSpec`, and the typed lifecycle for assigning /
/// swapping runtimes per site.
pub mod per_site_php_runtime;
/// Quotas bounded context: per-user/per-site disk, bandwidth,
/// inode, max-file-size, and CPU-share limits with soft/hard
/// pairs and grace windows. Enforcement is owned by the
/// application layer's typed ports.
pub mod quotas;
/// Service manager bounded context: allow-listed systemctl
/// surface with audited lifecycle actions.
pub mod service_manager;
/// SFTP / jailed shells bounded context: per-site SSH/SFTP
/// grants that map onto OpenSSH's `internal-sftp` + `ForceCommand`.
pub mod sftp_jailed_shells;
/// Per-site staging slots, sync policies, and atomic promote.
pub mod site_staging;
/// Synthetic monitoring bounded context: periodic HTTP / TCP / SSL
/// checks with a per-check throttle and typed alert decision.
pub mod synthetic_monitoring;
/// Wildcard SSL with DNS-01 challenge bounded context: cert
/// request with `ChallengeKind::Dns01`, ACME endpoint mode, and
/// a DNS lease lifecycle.
pub mod wildcard_ssl;
/// WordPress toolkit bounded context: staging, clone, update
/// (with rollback on failure), security scan, and cache layer.
pub mod wordpress_toolkit;

/// Internationalization: locale, catalog, locale negotiation, formatter.
pub mod i18n;
/// Observability export: Prometheus metrics, OTLP traces, JSONL logs.
pub mod observability_export;

pub use account_hierarchy::{
    AccountHierarchyError, AccountRelationship, HierarchyNode, HierarchyRepository,
    HierarchyStatus, PoolAxis, PoolClaim, QuotaPool, check_pool_claim, would_cycle,
};
pub use admin_ip_allowlist::{
    AdminIpAllowlist, AdminIpAllowlistError, AllowlistMode, AllowlistOverride, IpCidr,
};
pub use agent::{
    AgentError, AgentId, AgentRegistration, AgentRepository, AgentStatus, FleetToken,
    FleetTokenScope, RecipeAction, RecipeManifest, RecipeManifest as _RecipeManifest,
    is_manifest_signature_valid,
};
pub use ai_ops::{
    AiAction, AiActionId, AiActionStatus, AiMessage, AiOpsError, AiOpsRepository, AiSession,
    AiSessionId, MessageRole, ToolCallAllowlist, ToolKind, ToolName, ToolResult, ToolSpec,
};
pub use api_tokens::{
    ApiToken, ApiTokenError, ApiTokenMetadata, ApiTokenRepository, Cidr, TokenCredential,
    TokenHash, TokenScope,
};
pub use app_runtimes::{
    ALLOWED_RUNTIME_KINDS, RESERVED_PORTS, RuntimeError, RuntimeKind, RuntimeRepository,
    RuntimeStatus, SiteRuntime, is_kind_allowed, is_port_allowed, is_version_allowed,
    is_workdir_inside_chroot, render_nginx_proxy_block, render_supervisor_unit,
};
pub use bandwidth_accounting::{
    BandwidthCounterRow, BandwidthReader, BandwidthRepository, BandwidthStorageObserver,
};
pub use billing::{
    BillingError, BillingRepository, BillingStatus, Chargeback, ChargebackLine, Integration,
    UsageMeter, UsageUnit, compute_chargeback, hmac_sha256_hex, verify_signature,
};
pub use cluster_data_model::{
    ClusterError, ClusterNode, ClusterRepository, ClusterTopology, FailoverPolicy, NodeId,
    NodeRole, ReplicatedDatabase, ReplicationMode, SharedStorage, SharedStorageKind, StorageId,
};
pub use collaborators::{
    CollabStatus, Collaborator, CollaboratorError, CollaboratorId, CollaboratorRepository,
    Permission, PermissionSet, SiteGrant, SiteGrantRepository,
};
pub use common::{
    Email, Password, PasswordError, Username, UsernameError,
    error::{DomainError, RepoError},
};
pub use compliance::{
    AuditRetentionPolicy, ComplianceError, ComplianceRepository, GdprApiTokenRecord,
    GdprDatabaseRecord, GdprExport, GdprExportPayload, GdprMailRecord, GdprSiteRecord,
    HardeningRule, HardeningRun, REDACTED, RuleOutcome,
};
pub use container_registry::{
    ImageDigest, ImageNamespace, NamespaceId, RegistryConfig, RegistryError, RetentionPolicy,
    RetentionVerdict, ScanFinding as ContainerScanFinding, ScanResult, ScanStatus, StoredImage,
};
pub use container_runtime::{
    ContainerMetrics, ContainerQuota, ContainerRuntimeError, ContainerRuntimeRepository,
    EffectiveQuota, NetworkEgressAccount, PlanQuotaCaps, QuotaAxis, RegistryCredential,
    RegistryCredentialId, check_concurrent, check_cpu, check_egress, check_memory, check_total,
    effective_quota, egress_threshold_crossed,
};
pub use databases::{
    database::Database, engine::DatabaseEngine, error::DatabaseError,
    repository::DatabaseRepository, status::DatabaseStatus,
};
pub use db_pitr::{
    BinlogRange, BinlogSegment, BinlogSink, BinlogStream, BinlogStreamRepository,
    BinlogStreamStatus, DatabaseLookup, IncrementalBackup, IncrementalMode, IncrementalRepository,
    LogSeq, LogTailer, PitrError, PitrRepository, PitrRestore, PitrRestoreStatus, ReplayOutcome,
    RestoreReplayWindow, RestoreTimestamp, StreamTargetId,
};
pub use db_privileges::{
    AdminToolSession, DbGrant, DbPrivilegeError, DbPrivilegeRepository, GrantScope, Privilege,
    RemoteAccess,
};
pub use dnssec_secondary::{
    DnsSecError, DnsSecPolicy, DnsSecRepository, DsRecord, GlueRecord, KeyRole, KskRolloverState,
    SecondaryNs, SigningAlgorithm, ZoneSigningKey,
};
pub use files::{
    error::FileError,
    file_info::FileInfo,
    path::Path,
    repository::{FileRepository, MAX_READ_BYTES},
};
pub use git_deployment::{
    DeployError, DeployRepo, DeployRepository, DeployRun, DeployStatus, WebhookSecret,
    commit_placeholder, verify_webhook,
};
pub use hosting::HostingPlanId;
pub use hosting_plans::{
    AppId, EffectiveQuotas, HostedPhpRuntimeRef, HostingPlan, HostingPlanRepository,
    HostingPlansError, PlanAssignment, PlanFeature, PlanFeatureState, PlanId, PlanPrice,
    PlanQuotas, PlanResolver, PlanStatus,
};
pub use iac::{
    ApiContract, IacError, Language, Operation, OperationId, ProviderResource, ResourceEndpoint,
    ResourceKind, SdkPackage, SdkSurface, TerraformProvider,
};
pub use identity::{
    error::IdentityError,
    remember::{
        REMEMBER_DEVICE_COOKIE, REMEMBER_DEVICE_DEFAULT_LIFETIME, REMEMBER_DEVICE_VERSION,
        RememberedDevicePayload, hash_user_agent, ip_prefix,
    },
    repository::{SessionRepository, UserRepository},
    role::Role,
    session::{Session, SessionBuilder, SessionToken, SessionTokenError},
    user::User,
};
pub use ip_allocation::{
    IpAllocation, IpError, IpFamily, IpPool, IpRepository, IpStatus, PoolKind, SiteAddress,
    validate_cidr,
};
pub use kernel_isolation::{
    CgroupLimit, IsolationError, IsolationPolicy, IsolationRepository, ROOT_CGROUP,
    UserNamespaceConfig, user_cgroup_path,
};
pub use load_balancing::{
    HealthCheck, HealthProbe, LbError, LbRepository, LbStatus, Member, Pool, PoolAlgorithm,
    ProbeDecision, next_member,
};
pub use log_viewer::{
    LogAuthorization, LogDownloadRecord, LogDownloadRepository, LogLine, LogPage, LogQuery,
    LogRange, LogReader, LogSource, LogViewerError, RbacLogAuthorization,
};
pub use mail_filtering::{
    AntiSpamPolicy, AutoResponder, AutoResponderMode, CatchAll, Forwarder, GreylistEntry,
    MailFilterError, MailFilterRepository, MailingList, SIEVE_MAX_BYTES, SieveScript,
};
pub use maintenance_windows::{
    DestructiveActionClass, MaintenanceError, MaintenanceOverride, MaintenanceRepository,
    MaintenanceWindow,
};
pub use malware_scanner::{
    MalwareScannerError, MalwareScannerRepository, OnInfection, QuarantineRecord, RunStatus,
    ScanEngine, ScanFinding, ScanProfile, ScanRun, Severity,
};
pub use migration_importers::{
    DriverKind, ImportConflict, ImportedResource, ImportedResourceKind, MigrationDriver,
    MigrationError, MigrationPlan, MigrationPlanId, MigrationRepository, MigrationRun,
    MigrationRunId, MigrationRunStatus, MigrationSource, MigrationWarning, PlannedResource,
    SharedMigrationDriver, TranslationLog, TranslationLogEntry, TranslationOutcome,
};
pub use monitoring::{
    Alert, AlertRule, DiskReading, MetricKind, MetricSample, MonitoringError, NetworkReading,
    SnapshotRepository, SystemSnapshot, Unit,
};
pub use offsite_backup_targets::{
    BackupCredential, BackupTargetAdapter, CredentialKind, KekRef, OffsiteBackupError,
    OffsiteBackupRepository, RemoteTargetConfig,
};
pub use os_updates::{
    OsUpdateError, OsUpdateRepository, PackageUpdate, RebootState, UpdateHistoryRecord, UpdateKind,
    UpdatePolicy,
};
pub use per_site_php_runtime::{
    PhpFpmPoolSpec, PhpRuntimeError, PhpRuntimeRef, PhpRuntimeRepository, PhpRuntimeStatus,
};
pub use plugin::{
    Capability, CapabilitySet, ManifestRuntime, PluginError, PluginId, PluginManifest,
    PluginRecord, PluginRegistry, PluginStatus, PluginVersion, PublisherKey,
};
pub use plugin_marketplace::{
    CatalogCache, CatalogSnapshot, MarketplaceCa, MarketplaceCatalog, MarketplacePlugin,
    PluginMarketplaceError, PluginRating, PublisherSignature, SignedCatalogEnvelope,
    verify_envelope,
};
pub use quotas::{
    QuotaDimension, QuotaError, QuotaLimit, QuotaPolicy, QuotaRepository, QuotaSubject, QuotaUsage,
};
pub use service_manager::{
    DEFAULT_ALLOWLIST, ServiceAction, ServiceActionRecord, ServiceError, ServiceInfo,
    ServiceManagerRepository, ServiceStatus, is_allowed,
};
pub use sftp_jailed_shells::{
    JailPublicKey, JailedShellStatus, SftpJailError, SftpJailGrant, SftpJailRepository,
    render_sshd_config,
};
pub use site_cache_cdn::{
    CacheLevel, CdnAdapter, CdnIntegration, CdnKind, CdnZone, HeaderSummary, PurgeReceipt,
    PurgeRequest, SiteCacheCdnError, SiteCacheCdnRepository, SiteCachePolicy,
};
pub use site_clone_template::{
    AnonymisationToken, CloneFile, ClonePlan, CloneRun, CloneSource, DbAction, PiiPolicy,
    SiteCloneTemplateError, SiteCloneTemplateRepository, SiteTemplate,
};
pub use site_staging::{
    PromotionRepository, PromotionRun, PromotionStatus, SiteStagingError, SnapshotId, StagingSlot,
    StagingSlotRepository, StagingSnapshotRepository, SyncMode, SyncPolicy,
};
pub use sites::{error::SiteError, repository::SiteRepository, site::Site, status::SiteStatus};
pub use ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
    repository::CertificateRepository,
    source::{CertificateSource, CertificateStatus},
};
pub use synthetic_monitoring::{
    CheckResult, CheckStatus, CheckType, SyntheticCheck, SyntheticError, SyntheticRepository,
    classify,
};
pub use themeable_ui::{
    BrandingScope, HexColor, Palette, PanelDomain, ThemeOverride, ThemeableUiError,
    ThemeableUiRepository, Typography, contrast_ratio,
};
pub use waf::{Rule, RuleSet, WafError, WafRepository};
pub use web_application_installer::{
    CONFIRM_WINDOW_SECONDS, IDEMPOTENCY_TTL_SECONDS, IdempotencyKey, InstallArtifact, InstallDb,
    InstallOverlay, InstallPlan, InstallRun, InstalledWebApp, MAX_PLAN_VALIDITY_SECONDS,
    WebApplicationInstallerError, WebApplicationInstallerRepository,
};
pub use webmail_client::{
    MailBridge, MessageBody, MessageHeader, SESSION_TOKEN_TTL_SECONDS, WebmailError,
    WebmailRepository, WebmailSessionToken,
};
pub use wildcard_ssl::{
    ALLOWED_DNS_PROVIDERS, AcmeEndpointMode, CHALLENGE_SERVER_BIND, CertRequest, ChallengeKind,
    DnsLease, DnsProviderPort, RecordingDnsProvider, WildcardError, WildcardRepository,
    is_provider_allowed,
};
pub use wordpress_toolkit::{
    WpCacheMode, WpError, WpRepository, WpSecurityFinding, WpSecurityReport, WpSite,
    WpUpdateResult, WpUpdateSet, compare_versions,
};
