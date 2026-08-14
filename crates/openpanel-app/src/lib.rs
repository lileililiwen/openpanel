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
pub mod prelude;
pub mod security;
pub mod sites;
pub mod software_center;
pub mod ssl;
pub mod system_services;
pub mod waf;

pub use api_tokens::{ApiTokenModule, ApiTokenService};
pub use backups::{BackupService, BackupsModule};
pub use cron::{CronModule, CronService};
pub use databases::{DatabasesModule, service::DatabasesService};
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
pub use security::{SecurityModule, SecurityService};
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
