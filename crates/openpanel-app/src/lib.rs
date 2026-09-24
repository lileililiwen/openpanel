//! Application layer: use-case services and SQLite repository adapters.
//! Concrete modules (IdentityModule, etc.) live here. Each implements the
//! `Module` trait from `openpanel-core` and registers its services,
//! routes, CLI, and migrations.

#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

/// Account hierarchy bounded context: parent-child relationships,
/// tree traversal, cycle detection, and pooled quota caps.
pub mod account_hierarchy;
/// Agent bounded context: per-host agent registry, fleet tokens,
/// and signed recipe manifests.
pub mod agent;
/// AI Ops bounded context: conversational agent with tool-call
/// allowlist and human-in-the-loop approval gate.
pub mod ai_ops;
pub mod api_tokens;
/// Non-PHP runtime bounded context: per-site runtime choice
/// (Node / Python / Go / Ruby / .NET), pinned version, app port,
/// supervisor unit, and the nginx reverse-proxy block that
/// targets `127.0.0.1:APP_PORT`.
pub mod app_runtimes;
pub mod backups;
/// Reseller billing integration bounded context: usage meters,
/// chargeback pricing, integration state, and webhook HMAC
/// verification.
pub mod billing;
/// Cluster data model bounded context: typed host roles, shared
/// storage, and replicated database metadata.
pub mod cluster_data_model;
/// Per-site collaborator bounded context.
pub mod collaborators;
/// Compliance bounded context: CIS hardening, audit retention,
/// GDPR export with secret redaction.
pub mod compliance;
/// Container registry bounded context.
pub mod container_registry;
/// Container runtime bounded context: per-user quota, registry
/// credentials, metrics, and monthly egress accounting.
pub mod container_runtime;
pub mod cron;
pub mod databases;
/// Database point-in-time recovery: continuous binlog streaming,
/// point-in-time restore, and incremental file-backup deltas.
pub mod db_pitr;
/// Database privilege management bounded context: per-user grant
/// scopes, remote access with an explicit wildcard opt-in, and
/// short-lived single-use SSO tokens for the admin tool launcher.
pub mod db_privileges;
pub mod deliverability;
/// Deployment adapters bounded context: provider-neutral action
/// lifecycle, idempotency, evidence schema, and the
/// `DeploymentAdapterService` that the API, the CLI, and the
/// integration tests share. Concrete adapters (and the
/// conformance fixture) live outside this crate's product code.
pub mod deployment_adapters;
pub mod dns;
/// DNSSEC + secondary DNS bounded context: zone signing keys,
/// secondary nameserver ACLs, glue records, and DS records.
pub mod dnssec_secondary;
pub mod docker;
/// Feedback widget submissions: SQLite repository, service, module.
pub mod feedback;
pub mod files;
pub mod ftp;
/// Git deployment bounded context: a per-site git repo, deploy
/// runs, and webhook HMAC verification.
pub mod git_deployment;
/// Hosting plans bounded context: plan definitions, quota caps,
/// feature toggles, the read-side resolver, and the assignment
/// table that backs `User.hosting_plan_id`.
pub mod hosting_plans;
/// Internationalization bounded context: `LocaleService`, catalog
/// resolution, locale-aware formatting, and per-user preferences.
pub mod i18n;
/// Infrastructure-as-Code bounded context.
pub mod iac;
pub mod identity;
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
pub mod logs;
pub mod mail;
/// Mail anti-spam and filtering bounded context: per-mailbox
/// anti-spam policy, greylist, Sieve filter scripts, autoresponder
/// windows, forwarders, catch-all, and mailing lists.
pub mod mail_filtering;
/// Scheduled maintenance windows bounded context: a panel-wide
/// schedule that blocks destructive actions, with a
/// single-use override that lifts the lock for a bounded TTL.
pub mod maintenance_windows;
/// Web application malware scanner bounded context: scan
/// profiles, runs, findings, and quarantine.
pub mod malware_scanner;
/// Migration importers bounded context: cPanel / Baota backup
/// import pipelines with preview, atomic run, and rollback.
pub mod migration_importers;
pub mod migrations;
pub mod monitoring;
pub mod monitoring_fleet;
pub mod notifications;
/// Offsite backup targets bounded context: encrypted credential
/// lifecycle, KEK management, remote target attachment, and the
/// upload service driving `BackupTargetAdapter` implementations.
pub mod offsite_backup_targets;
/// Operator security control plane: normalized finding lifecycle,
/// typed remediation adapters, expiry, and verification.
pub mod operator_security;
/// OS update management bounded context: package updates,
/// unattended-upgrades policy, reboot state.
pub mod os_updates;
/// Plugin extension framework bounded context.
pub mod plugin;
/// Plugin marketplace bounded context.
pub mod plugin_marketplace;
pub mod prelude;
/// Quotas bounded context: per-user / per-site resource quotas
/// with soft/hard limits and grace windows.
pub mod quotas;
/// Release preflight: refuse to mutate a store whose schema is
/// newer than this binary supports. Decision logic is pure; the
/// async composition-root glue lives in the composition-root crate.
pub mod release_preflight;
pub mod security;
/// Service manager bounded context: allow-listed systemctl
/// surface with audited lifecycle actions.
pub mod service_manager;
/// Site cache and CDN integration bounded context: per-site cache
/// policies, CDN integrations, nginx snippet generation, and purge
/// orchestration.
pub mod site_cache_cdn;
/// Site clone + template export bounded context: clone plans,
/// clone runs, template export, and PII anonymisation tokens.
pub mod site_clone_template;
pub mod site_http_controls;
/// Per-site staging bounded context: staging slot creation, sync,
/// and atomic promote.
pub mod site_staging;
pub mod sites;
pub mod software_center;
pub mod ssl;
/// Synthetic monitoring bounded context: periodic HTTP / TCP / SSL
/// checks with a per-check throttle and typed alert decision.
pub mod synthetic_monitoring;
pub mod system_services;
/// Themeable UI and white-label bounded context: per-account
/// theme overrides, palette validation, logo upload, and host
/// resolution.
pub mod themeable_ui;
pub mod waf;
/// Web application installer bounded context: typed install
/// plans, install runs, idempotency, and uninstall.
pub mod web_application_installer;
pub mod web_terminal;
/// Webmail client bounded context: session tokens, bridge, and
/// redaction.
pub mod webmail_client;
/// Wildcard SSL with DNS-01 challenge bounded context: cert
/// request with `ChallengeKind::Dns01`, ACME endpoint mode, and
/// a DNS lease lifecycle.
pub mod wildcard_ssl;
/// WordPress toolkit bounded context: staging, clone, update
/// (with rollback on failure), security scan, and cache layer.
pub mod wordpress_toolkit;

pub use account_hierarchy::{AccountHierarchyModule, HierarchyService, SqliteHierarchyRepository};
pub use agent::{AgentModule, AgentService, SqliteAgentRepository};
pub use ai_ops::{
    ActionApproval, AiOpsModule, AskService, SqliteAiOpsRepository, ToolExecutor, default_allowlist,
};
pub use api_tokens::{ApiTokenModule, ApiTokenService};
pub use app_runtimes::{
    AppRuntimesModule, ReverseProxyLayer, RuntimeEnvService, RuntimeService,
    SqliteRuntimeRepository, SupervisorUnitBuilder,
};
pub use backups::{
    BackupService, BackupsModule,
    server_snapshot::{ServerSnapshotError, ServerSnapshotService},
    snapshot_importer::{
        SkipAll, SnapshotBundleSource, SnapshotImporterDriver, SnapshotTranslator,
        export_to_target, import_from_target,
    },
};
pub use billing::{
    BillingModule, BillingService, ChargebackEngine, SqliteBillingRepository, UsageExporter,
    WebhookRelay,
};
pub use cluster_data_model::{ClusterDataModelModule, ClusterService, SqliteClusterRepository};
/// Per-site collaborator module.
pub use collaborators::{
    CollaboratorService, CollaboratorsModule, GrantResolver, InviteCollaboratorError,
    InviteRequest, SqliteCollaboratorRepository, SqliteSiteGrantRepository, UpdateRequest,
};
pub use compliance::{
    AuditRetentionService, ComplianceModule, GdprExporter, HardeningWizard,
    SqliteComplianceRepository, default_profile,
};
/// Container registry module.
pub use container_registry::{
    ContainerRegistryModule, ContainerRegistryService, ImageBlob, NoopScanHook, PushError,
    PushRequest, PushResult, ScanHook, ScanHookError, SqliteImageRepository,
    SqliteNamespaceRepository, SqliteScanResultRepository,
};
/// Container runtime module.
pub use container_runtime::{
    ContainerRuntimeModule, ContainerRuntimeService, CreateCredentialResult, PullError,
    PullRequest, PullResult, RegistryHostAdapter,
};
pub use cron::{CronModule, CronService};
pub use databases::{DatabasesModule, service::DatabasesService};
/// Database point-in-time recovery bounded-context module.
pub use db_pitr::{DbPitrModule, PitrService};
/// In-memory `BinlogSink` / `LogTailer` adapters for tests and
/// offline development.
pub use db_pitr::{InMemoryBinlogSink, InMemoryLogTailer};
pub use db_privileges::{
    AdminToolSso, DbPrivilegeModule, DbRemoteAccessContext, MemoryGrantPort, MySqlGrantPort,
    MySqlShellGrantPort, PrivilegeService, RemoteAccessController, SqliteDbPrivilegeRepository,
};
pub use deliverability::{DeliverabilityModule, DeliverabilityService};
/// Deployment-adapter service (idempotency, evidence, audit
/// fan-out). The Mac/Jenkins conformance fixture and any
/// production adapter implementations live outside this
/// crate's product code.
pub use deployment_adapters::{DeploymentAdapterService, DeploymentServiceError};
pub use dns::{DnsModule, DnsService};
pub use dnssec_secondary::{
    AxfrSender, DnsSecSecondaryModule, DnsSecService, GlueRecordService, KeyRolloverEngine,
    RecordingRegistrar, Registrar, SqliteDnsSecRepository,
};
pub use docker::{
    ApplyReport, BollardDockerAdapter, DockerAdapter, DockerModule, DockerService, ExecResult,
    NetworkAdapter, RuntimeContainerState, SqliteDockerRepository,
};
/// Feedback widget module.
pub use feedback::{FeedbackModule, FeedbackService, SqliteFeedbackRepository};
pub use files::{FilesModule, service::FilesService};
pub use ftp::{
    ChrootStorage, CreateFtpAccount, CreatedFtpAccount, FtpAccountView, FtpAuthenticator,
    FtpModule, FtpServerConfig, FtpServerTask, FtpService, FtpSessionRegistry, SqliteFtpRepository,
    UpdateFtpAccount,
};
pub use git_deployment::{
    DeployService, GitDeploymentModule, PreviewService, SqliteDeployRepository,
    SqlitePreviewRepository, WebhookVerifier, verify_webhook_with_secret,
};
pub use hosting_plans::{HostingPlansModule, HostingPlansService, SqliteHostingPlanRepository};
pub use i18n::{
    I18nAppError, LocaleService, SqliteLocaleUserPrefsRepository, TranslationEntry, default_catalog,
};
/// Infrastructure-as-Code module.
pub use iac::{
    CodegenContract, CommittedArtifacts, DriftOutcome, GeneratedSurface, OpenApiRef, ParsedOpenApi,
    render_go_stub, render_provider_stub, render_rust_stub, render_typescript_stub,
};
pub use identity::{
    IdentityModule,
    service::IdentityService,
    sso::{CallbackOutcome, OpenidConnectAdapter, SsoModule, SsoService},
};
pub use ip_allocation::{
    Allocator, IpAllocationModule, IpService, SqliteIpRepository, VhostBinder,
};
pub use kernel_isolation::{
    CgroupEnforcer, CgroupWriter, KernelIsolationModule, NamespaceIsolator, QuotaBridge,
    RecordingCgroupWriter, SqliteIsolationRepository,
};
pub use load_balancing::{
    LbService, LoadBalancingModule, MemberRotator, RecordingHealthProbe, SqliteLbRepository,
};
pub use log_viewer::{
    InMemoryLogReader, LogAggregator, LogViewerModule, SqliteLogViewerRepository,
};
pub use logs::{LogRotationService, LogService, LogsModule};
pub use mail::{MailModule, MailService};
pub use mail_filtering::{
    MailFilterService, MailFilteringModule, MailingListService, SieveCompiler, SpamScorer,
    SqliteMailFilterRepository,
};
pub use maintenance_windows::{
    MaintenanceEnforcer, MaintenanceWindowsModule, SqliteMaintenanceRepository,
};
pub use malware_scanner::{
    DEFAULT_QUARANTINE_ROOT, MalwareScannerService, RESTORE_WINDOW_SECONDS, RealScannerFs,
    ScannerFs, SqliteMalwareScannerRepository,
};
pub use migration_importers::{
    JsonManifest, JsonManifestBundle, ManifestResource, ManifestTranslator,
    MigrationImportersModule, MigrationService, RefuseAll, SqliteMigrationRepository,
    TarWithJsonManifestDriver, sniff_tar_manifest,
};
pub use monitoring::{MonitoringModule, service::MonitoringService};
pub use monitoring_fleet::{FleetServiceError, MonitoringFleetService};
pub use notifications::{NotificationModule, NotificationService};
pub use offsite_backup_targets::{
    BackupUploadService, OffsiteBackupTargetsModule, SqliteOffsiteBackupRepository,
    decrypt_payload, decrypt_payload as kek_decrypt, derive_kek, encrypt_payload,
    encrypt_payload as kek_encrypt, master_key_fingerprint, unwrap_kek, wrap_kek,
};
/// Re-export the `BinlogSink` trait so composition code can name it.
pub use openpanel_domain::BinlogSink;
pub use operator_security::{
    ControlPlaneServiceError, ControlRemediationKind, ControlRemediationPort,
    ControlRemediationPreview, ExistingServiceRemediationPort, OperatorSecurityService,
};
pub use os_updates::{
    OsUpdateApplier, OsUpdateLister, OsUpdateModule, PackageManager, RecordingPackageManager,
    SqliteOsUpdateRepository, UnattendedConfig,
};
/// Plugin extension framework module.
pub use plugin::{PluginModule, PluginService, SqlitePluginRegistry};
/// Plugin marketplace module.
pub use plugin_marketplace::{
    DiscoverOutcome, HttpMarketplaceClient, InstallFromMarketplaceError,
    InstallFromMarketplaceRequest, MarketplaceClient, MarketplaceService, MockMarketplaceClient,
    PluginMarketplaceModule, SqliteCatalogCache,
};
pub use quotas::{QuotaService, QuotasModule, SqliteQuotaRepository};
pub use release_preflight::{PreflightError, PreflightOutcome, evaluate, evaluate_runner};
pub use security::{HostSshKeysService, SecurityModule, SecurityService};
pub use service_manager::{
    RecordingSystemCtl, ServiceActor, ServiceLister, ServiceManagerModule,
    SqliteServiceManagerRepository,
};
pub use site_cache_cdn::{
    ApplyOutcome, CdnAdapterRegistry, CdnProviderConfig, CloudFrontAdapter, CloudflareAdapter,
    GenericHttpAdapter, NginxApplyError, NginxCacheManager, PurgeSummary, SiteCacheCdnModule,
    SiteCacheService, SqliteSiteCacheCdnRepository, cache_directives, cache_path_directive,
    keys_zone_name, provider_config_for,
};
pub use site_clone_template::{
    FileEnumerator, FsEnumerator, MODULE_NAME as SITE_CLONE_TEMPLATE_MODULE, SiteCloneService,
    SiteCloneTemplateModule, SqliteSiteCloneTemplateRepository, TemplateArtifact, TemplateExporter,
    default_deny_patterns,
};
pub use site_http_controls::{SiteHttpControlsModule, SiteHttpService};
/// Per-site staging bounded-context module.
pub use site_staging::{
    InMemoryStagingFilesystem, SiteStagingModule, StagingFilesystemLayer, StagingService,
};
pub use sites::{
    SitesModule,
    nginx::{NginxConfigGenerator, NginxPaths},
    service::SitesService,
    transport_service::SiteTransportService,
};
pub use software_center::{SoftwareCenterModule, SoftwareCenterService};
pub use ssl::{
    AcmeEndpoint, SslModule, SslPaths, SslService, challenge_server::AcmeHttpServer,
    module::CHALLENGE_SERVER_PORT,
};
pub use synthetic_monitoring::{
    CheckRunner, NativeTlsSslInspector, ProbeScheduler, RecordingProbe, ReqwestHttpProbe,
    SqliteStatusPageRepository, SqliteSyntheticRepository, StatusPageService,
    SyntheticMonitoringModule, TokioTcpProbe,
};
pub use system_services::{ServiceManager, SystemServicesModule};
pub use themeable_ui::{
    DEFAULT_BRANDING_ROOT, MAX_LOGO_BYTES, SqliteThemeableUiRepository, ThemeableUiService,
};
pub use waf::{WafModule, WafService};
pub use web_application_installer::{
    ArtifactDownloader, InstallerFs, RealInstallerFs, ReqwestArtifactDownloader,
    SqliteWebApplicationInstallerRepository, WebApplicationInstallerService,
};
pub use web_terminal::{WebTerminalModule, WebTerminalService};
pub use webmail_client::{
    InMemoryMailBridge, SqliteWebmailRepository, WebmailService, redact_html, sha256_hex,
};
pub use wildcard_ssl::{
    CertRenewalScheduler, Dns01ChallengeSolver, RecordingDnsProvider, SqliteWildcardRepository,
    WildcardIssuer, WildcardSslModule,
};
pub use wordpress_toolkit::{
    FakeWpFilesystem, SqliteWpRepository, WordPressToolkitModule, WpCacheLayer, WpScanner,
    WpToolkitService, WpUpdater,
};

/// Generate a random URL-safe token (32 bytes of entropy).
pub fn random_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}
