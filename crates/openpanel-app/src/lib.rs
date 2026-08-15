//! Application layer: use-case services and SQLite repository adapters.
//! Concrete modules (IdentityModule, etc.) live here. Each implements the
//! `Module` trait from `openpanel-core` and registers its services,
//! routes, CLI, and migrations.

#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod api_tokens;
pub mod backups;
pub mod cron;
pub mod databases;
/// Database point-in-time recovery: continuous binlog streaming,
/// point-in-time restore, and incremental file-backup deltas.
pub mod db_pitr;
pub mod dns;
pub mod docker;
pub mod files;
pub mod ftp;
/// Internationalization bounded context: `LocaleService`, catalog
/// resolution, locale-aware formatting, and per-user preferences.
pub mod i18n;
pub mod identity;
pub mod logs;
pub mod mail;
pub mod migrations;
pub mod monitoring;
pub mod notifications;
/// Plugin extension framework bounded context.
pub mod plugin;
/// Plugin marketplace bounded context.
pub mod plugin_marketplace;
/// Per-site collaborator bounded context.
pub mod collaborators;
/// Container registry bounded context.
pub mod container_registry;
/// Infrastructure-as-Code bounded context.
pub mod iac;
pub mod prelude;
pub mod security;
/// Per-site staging bounded context: staging slot creation, sync,
/// and atomic promote.
pub mod site_staging;
pub mod sites;
pub mod software_center;
pub mod ssl;
pub mod system_services;
pub mod waf;
/// AI Ops bounded context: conversational agent with tool-call
/// allowlist and human-in-the-loop approval gate.
pub mod ai_ops;
/// Compliance bounded context: CIS hardening, audit retention,
/// GDPR export with secret redaction.
pub mod compliance;
/// Service manager bounded context: allow-listed systemctl
/// surface with audited lifecycle actions.
pub mod service_manager;
/// OS update management bounded context: package updates,
/// unattended-upgrades policy, reboot state.
pub mod os_updates;
/// Synthetic monitoring bounded context: periodic HTTP / TCP / SSL
/// checks with a per-check throttle and typed alert decision.
pub mod synthetic_monitoring;
/// Log viewer bounded context: typed queries over a JSONL store
/// with role-based authorization.
pub mod log_viewer;
/// Database privilege management bounded context: per-user grant
/// scopes, remote access with an explicit wildcard opt-in, and
/// short-lived single-use SSO tokens for the admin tool launcher.
pub mod db_privileges;
/// IPv6 + address-pool bounded context: typed pools, allocations
/// to sites, and the vhost binder that attaches the address set
/// to a vhost.
pub mod ip_allocation;
/// Reseller billing integration bounded context: usage meters,
/// chargeback pricing, integration state, and webhook HMAC
/// verification.
pub mod billing;
/// Load balancing and failover bounded context: pools of
/// members with health probes and weighted rotation.
pub mod load_balancing;
/// WordPress toolkit bounded context: staging, clone, update
/// (with rollback on failure), security scan, and cache layer.
pub mod wordpress_toolkit;
/// Wildcard SSL with DNS-01 challenge bounded context: cert
/// request with `ChallengeKind::Dns01`, ACME endpoint mode, and
/// a DNS lease lifecycle.
pub mod wildcard_ssl;
/// Non-PHP runtime bounded context: per-site runtime choice
/// (Node / Python / Go / Ruby / .NET), pinned version, app port,
/// supervisor unit, and the nginx reverse-proxy block that
/// targets `127.0.0.1:APP_PORT`.
pub mod app_runtimes;
/// Kernel resource isolation bounded context: per-user cgroup
/// limits and namespace configuration.
pub mod kernel_isolation;
/// DNSSEC + secondary DNS bounded context: zone signing keys,
/// secondary nameserver ACLs, glue records, and DS records.
pub mod dnssec_secondary;
/// Mail anti-spam and filtering bounded context: per-mailbox
/// anti-spam policy, greylist, Sieve filter scripts, autoresponder
/// windows, forwarders, catch-all, and mailing lists.
pub mod mail_filtering;
/// Git deployment bounded context: a per-site git repo, deploy
/// runs, and webhook HMAC verification.
pub mod git_deployment;
/// Scheduled maintenance windows bounded context: a panel-wide
/// schedule that blocks destructive actions, with a
/// single-use override that lifts the lock for a bounded TTL.
pub mod maintenance_windows;

pub use api_tokens::{ApiTokenModule, ApiTokenService};
pub use backups::{BackupService, BackupsModule};
pub use cron::{CronModule, CronService};
pub use databases::{DatabasesModule, service::DatabasesService};
/// Database point-in-time recovery bounded-context module.
pub use db_pitr::{DbPitrModule, PitrService};
/// In-memory `BinlogSink` / `LogTailer` adapters for tests and
/// offline development.
pub use db_pitr::{InMemoryBinlogSink, InMemoryLogTailer};
pub use dns::{DnsModule, DnsService};
pub use docker::{
    ApplyReport, BollardDockerAdapter, DockerAdapter, DockerModule, DockerService, ExecResult,
    NetworkAdapter, RuntimeContainerState, SqliteDockerRepository,
};
pub use files::{FilesModule, service::FilesService};
pub use ftp::{
    ChrootStorage, CreateFtpAccount, CreatedFtpAccount, FtpAccountView, FtpAuthenticator,
    FtpModule, FtpServerConfig, FtpServerTask, FtpService, FtpSessionRegistry, SqliteFtpRepository,
    UpdateFtpAccount,
};
pub use i18n::{
    I18nAppError, LocaleService, SqliteLocaleUserPrefsRepository, TranslationEntry, default_catalog,
};
pub use identity::{IdentityModule, service::IdentityService};
pub use logs::{LogService, LogsModule};
pub use mail::{MailModule, MailService};
pub use monitoring::{MonitoringModule, service::MonitoringService};
pub use notifications::{NotificationModule, NotificationService};
/// Plugin extension framework module.
pub use plugin::{PluginModule, PluginService, SqlitePluginRegistry};
/// Plugin marketplace module.
pub use plugin_marketplace::{
    DiscoverOutcome, HttpMarketplaceClient, InstallFromMarketplaceError,
    InstallFromMarketplaceRequest, MarketplaceClient, MarketplaceService, MockMarketplaceClient,
    PluginMarketplaceModule, SqliteCatalogCache,
};
/// Per-site collaborator module.
pub use collaborators::{
    CollaboratorsModule, CollaboratorService, GrantResolver, InviteCollaboratorError,
    InviteRequest, SqliteCollaboratorRepository, SqliteSiteGrantRepository, UpdateRequest,
};
/// Container registry module.
pub use container_registry::{
    ContainerRegistryModule, ContainerRegistryService, ImageBlob, NoopScanHook,
    PushError, PushRequest, PushResult, ScanHook, ScanHookError,
    SqliteImageRepository, SqliteNamespaceRepository, SqliteScanResultRepository,
};
/// Infrastructure-as-Code module.
pub use iac::{
    CodegenContract, CommittedArtifacts, DriftOutcome, GeneratedSurface, OpenApiRef,
    ParsedOpenApi, render_go_stub, render_provider_stub, render_rust_stub,
    render_typescript_stub,
};
/// Re-export the `BinlogSink` trait so composition code can name it.
pub use openpanel_domain::BinlogSink;
pub use security::{SecurityModule, SecurityService};
/// Per-site staging bounded-context module.
pub use site_staging::{
    InMemoryStagingFilesystem, SiteStagingModule, StagingFilesystemLayer, StagingService,
};
pub use sites::{
    SitesModule,
    nginx::{NginxConfigGenerator, NginxPaths},
    service::SitesService,
};
pub use software_center::{SoftwareCenterModule, SoftwareCenterService};
pub use ssl::{
    AcmeEndpoint, SslModule, SslPaths, SslService, challenge_server::AcmeHttpServer,
    module::CHALLENGE_SERVER_PORT,
};
pub use system_services::{ServiceManager, SystemServicesModule};
pub use waf::{WafModule, WafService};
pub use ai_ops::{AiOpsModule, AskService, ActionApproval, SqliteAiOpsRepository, ToolExecutor, default_allowlist};
pub use compliance::{
    ComplianceModule, AuditRetentionService, GdprExporter, HardeningWizard,
    SqliteComplianceRepository, default_profile,
};
pub use service_manager::{
    ServiceManagerModule, RecordingSystemCtl, ServiceActor, ServiceLister,
    SqliteServiceManagerRepository,
};
pub use os_updates::{
    OsUpdateModule, OsUpdateApplier, OsUpdateLister, PackageManager, RecordingPackageManager,
    SqliteOsUpdateRepository, UnattendedConfig,
};
pub use synthetic_monitoring::{
    SyntheticMonitoringModule, CheckRunner, ProbeScheduler, RecordingProbe,
    SqliteSyntheticRepository,
};
pub use log_viewer::{
    LogViewerModule, InMemoryLogReader, LogAggregator, SqliteLogViewerRepository,
};
pub use db_privileges::{
    DbPrivilegeModule, AdminToolSso, PrivilegeService, RemoteAccessController,
    SqliteDbPrivilegeRepository,
};
pub use ip_allocation::{
    IpAllocationModule, Allocator, IpService, SqliteIpRepository, VhostBinder,
};
pub use billing::{
    BillingModule, BillingService, ChargebackEngine, SqliteBillingRepository, UsageExporter,
    WebhookRelay,
};
pub use load_balancing::{
    LoadBalancingModule, LbService, MemberRotator, RecordingHealthProbe, SqliteLbRepository,
};
pub use wordpress_toolkit::{
    WordPressToolkitModule, FakeWpFilesystem, SqliteWpRepository, WpCacheLayer, WpScanner,
    WpToolkitService, WpUpdater,
};
pub use wildcard_ssl::{
    WildcardSslModule, CertRenewalScheduler, Dns01ChallengeSolver, RecordingDnsProvider,
    SqliteWildcardRepository, WildcardIssuer,
};
pub use app_runtimes::{
    AppRuntimesModule, ReverseProxyLayer, RuntimeService, SqliteRuntimeRepository,
    SupervisorUnitBuilder,
};
pub use kernel_isolation::{
    KernelIsolationModule, CgroupEnforcer, CgroupWriter, NamespaceIsolator, QuotaBridge,
    RecordingCgroupWriter, SqliteIsolationRepository,
};
pub use dnssec_secondary::{
    DnsSecSecondaryModule, AxfrSender, DnsSecService, GlueRecordService, KeyRolloverEngine,
    RecordingRegistrar, Registrar, SqliteDnsSecRepository,
};
pub use mail_filtering::{
    MailFilteringModule, MailFilterService, MailingListService, SieveCompiler, SpamScorer,
    SqliteMailFilterRepository,
};
pub use git_deployment::{
    GitDeploymentModule, DeployService, SqliteDeployRepository, WebhookVerifier,
    verify_webhook_with_secret,
};
pub use maintenance_windows::{
    MaintenanceWindowsModule, MaintenanceEnforcer, SqliteMaintenanceRepository,
};
