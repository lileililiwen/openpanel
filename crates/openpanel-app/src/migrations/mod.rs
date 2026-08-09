//! Embedded SQL migrations. Each module places its migration files under
//! `migrations/<module_name>/V###__*.sql`. The `apply` helper in core walks
//! the directory at runtime.

/// SQL for the audit-log v001 migration (schema for `audit_log`).
pub const AUDIT_V001: &str = include_str!("000_audit.sql");
/// SQL for the identity v001 migration (`users` and `sessions` tables).
pub const IDENTITY_V001: &str = include_str!("identity/V001__init.sql");
/// SQL for the sites v001 migration (`sites` table).
pub const SITES_V001: &str = include_str!("sites/V001__init.sql");
/// SQL for the databases v001 migration (`databases` table).
pub const DATABASES_V001: &str = include_str!("databases/V001__init.sql");
/// SQL for the ssl v001 migration (`certificates` table).
pub const SSL_V001: &str = include_str!("ssl/V001__init.sql");
/// SQL for the monitoring v001 migration (`monitoring_samples` table).
pub const MONITORING_V001: &str = include_str!("monitoring/V001__init.sql");
/// SQL for the cron v001 migration (`cron_jobs` and `cron_runs`).
pub const CRON_V001: &str = include_str!("cron/V001__init.sql");
/// SQL for backup plans, runs, and restore jobs.
pub const BACKUPS_V001: &str = include_str!("backups/V001__init.sql");
/// SQL for traffic aggregates and idempotent log offsets.
pub const LOGS_V001: &str = include_str!("logs/V001__init.sql");
/// SQL for managed firewall drafts, login failures, and temporary blocks.
pub const SECURITY_V001: &str = include_str!("security/V001__init.sql");
/// System-service health history schema.
pub const SYSTEM_SERVICES_V001: &str = include_str!("system_services/V001__init.sql");
