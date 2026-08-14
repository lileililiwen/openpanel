//! openpanel-core: cross-cutting primitives — Module trait, Config, Database,
//! AuditService, Job supervisor, tracing init.
//!
//! This crate is the composition root's toolbox. Domain code lives in
//! `openpanel-domain`; concrete modules (services + repos + migrations) live in
//! `openpanel-app`. Adapters (HTTP, CLI) sit on top.

#![deny(rustdoc::broken_intra_doc_links)]
#![allow(clippy::result_large_err)]

/// Audit event logging primitives and services.
pub mod audit;
/// Configuration loading, layering, and validation.
pub mod config;
/// Database driver abstraction (SQLite-backed in v0.1).
pub mod database;
/// Central error types for the core crate.
pub mod error;
/// Background job and task supervision.
pub mod jobs;
/// SQL migration discovery and application.
pub mod migration;
/// Module trait and registry for the composition root.
pub mod module;
/// Per-file line-count thresholds and exclude glob list.
pub mod quality;
/// Tracing subscriber initialization.
pub mod tracing_init;

/// Re-export of the audit event types and services.
pub use audit::{
    AuditAction, AuditEvent, AuditOutcome, AuditService, NoopAuditService, SqliteAuditService,
};
/// Re-export of the validated application config.
pub use config::Config;
/// Re-export of the database driver trait and SQLite implementation.
pub use database::{DatabaseDriver, SqliteDriver};
/// Re-export of the core error and result types.
pub use error::{ConfigError, CoreError, CoreResult};
/// Re-export of the background job and supervisor types.
pub use jobs::{BackgroundTask, Job, JobSupervisor, SupervisorHandle};
/// Re-export of the migration types.
pub use migration::{Migration, MigrationRecord, MigrationRunner};
/// Re-export of the module trait, registry, and context types.
pub use module::{AppContext, Module, ModuleRegistry, RouteMount};
/// Re-export of the file-length threshold value object.
pub use quality::{DEFAULT_HARD_LIMIT, DEFAULT_SOFT_LIMIT, FileLengthThresholds, LintExtraFile};
/// Re-export of the tracing initializer.
pub use tracing_init::init_tracing;
