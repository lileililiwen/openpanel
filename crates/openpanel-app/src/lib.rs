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
