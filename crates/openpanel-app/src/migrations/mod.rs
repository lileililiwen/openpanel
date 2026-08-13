//! Embedded SQL migrations. Each module places its migration files under
//! `migrations/<module_name>/V###__*.sql`. The `apply` helper in core walks
//! the directory at runtime.

/// SQL for the audit-log v001 migration (schema for `audit_log`).
pub const AUDIT_V001: &str = include_str!("000_audit.sql");
/// SQL for the identity v001 migration (`users` and `sessions` tables).
pub const IDENTITY_V001: &str = include_str!("identity/V001__init.sql");
/// SQL for the identity v002 migration (two-factor authentication:
/// `user_factors`, `recovery_codes`, `login_challenges`).
pub const IDENTITY_V002: &str = include_str!("identity/V002__2fa.sql");
/// SQL for the identity v003 migration (WebAuthn credentials and
/// in-flight ceremony challenges).
pub const IDENTITY_V003: &str = include_str!("identity/V003__webauthn.sql");
/// SQL for the identity v004 migration (adds `passkey_json` to
/// `webauthn_credentials` so the assertion ceremony can reconstruct
/// the webauthn-rs `Passkey` without decoding raw COSE bytes).
pub const IDENTITY_V004: &str = include_str!("identity/V004__webauthn_passkey_json.sql");
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
/// Provider accounts, zones, and synchronized records.
pub const DNS_V001: &str = include_str!("dns/V001__init.sql");
/// Hosted mail domains, mailboxes, aliases, and DKIM custody.
pub const MAIL_V001: &str = include_str!("mail/V001__init.sql");
/// Trusted Software Center catalogs, plans, jobs, locks, and deployments.
pub const SOFTWARE_CENTER_V001: &str = include_str!("software_center/V001__init.sql");
/// Normalized Software Center catalog tables for the aggregator model.
pub const SOFTWARE_CENTER_V002: &str = include_str!("software_center/V002__normalized.sql");
/// Adds `platforms_json` to `software_entries` for databases seeded
/// before the column existed (idempotent on newer installs).
pub const SOFTWARE_CENTER_V003: &str = include_str!("software_center/V003__platforms_backfill.sql");
/// Per-site WAF rule sets and aggregated hit rows.
pub const WAF_V001: &str = include_str!("waf/V001__init.sql");
