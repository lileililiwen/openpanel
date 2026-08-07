//! openpanel-core: cross-cutting primitives — Module trait, Config, Database,
//! AuditService, Job supervisor, tracing init.
//!
//! This crate is the composition root's toolbox. Domain code lives in
//! `openpanel-domain`; concrete modules (services + repos + migrations) live in
//! `openpanel-app`. Adapters (HTTP, CLI) sit on top.

#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::result_large_err)]

pub mod audit;
pub mod config;
pub mod database;
pub mod error;
pub mod jobs;
pub mod migration;
pub mod module;
pub mod tracing_init;

pub use audit::{
    AuditAction, AuditEvent, AuditOutcome, AuditService, NoopAuditService, SqliteAuditService,
};
pub use config::Config;
pub use database::{DatabaseDriver, SqliteDriver};
pub use error::{ConfigError, CoreError, CoreResult};
pub use jobs::{BackgroundTask, Job, JobSupervisor, SupervisorHandle};
pub use migration::{Migration, MigrationRecord, MigrationRunner};
pub use module::{AppContext, Module, ModuleRegistry, RouteMount};
pub use tracing_init::init_tracing;
