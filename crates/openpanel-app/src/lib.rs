//! Application layer: use-case services and SQLite repository adapters.
//! Concrete modules (IdentityModule, etc.) live here. Each implements the
//! `Module` trait from `openpanel-core` and registers its services,
//! routes, CLI, and migrations.

#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod backups;
pub mod cron;
pub mod databases;
pub mod files;
pub mod identity;
pub mod logs;
pub mod migrations;
pub mod monitoring;
pub mod prelude;
pub mod sites;
pub mod ssl;

pub use backups::{BackupService, BackupsModule};
pub use cron::{CronModule, CronService};
pub use databases::{DatabasesModule, service::DatabasesService};
pub use files::{FilesModule, service::FilesService};
pub use identity::{IdentityModule, service::IdentityService};
pub use logs::{LogService, LogsModule};
pub use monitoring::{MonitoringModule, service::MonitoringService};
pub use sites::{
    SitesModule,
    nginx::{NginxConfigGenerator, NginxPaths},
    service::SitesService,
};
pub use ssl::{
    AcmeEndpoint, SslModule, SslPaths, SslService, challenge_server::AcmeHttpServer,
    module::CHALLENGE_SERVER_PORT,
};
