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
/// Docker desired state, stacks, and trusted image patterns.
pub const DOCKER_V001: &str = include_str!("docker/V001__init.sql");
/// Per-site FTP accounts.
pub const FTP_V001: &str = include_str!("ftp/V001__init.sql");
/// Scoped personal access tokens.
pub const API_TOKENS_V001: &str = include_str!("api_tokens/V001__init.sql");
/// Durable notification channels, subscriptions, events, and deliveries.
pub const NOTIFICATIONS_V001: &str = include_str!("notifications/V001__init.sql");
/// Per-user locale preferences and the signed translation store.
pub const I18N_V001: &str = include_str!("i18n/V001__init.sql");
/// Database point-in-time recovery: binlog streams, restore jobs,
/// and incremental file-backup deltas.
pub const DB_PITR_V001: &str = include_str!("db_pitr/V001__init.sql");
/// Per-site staging slots, snapshots, and promotion runs.
pub const SITE_STAGING_V001: &str = include_str!("site_staging/V001__init.sql");
/// Plugin extension framework: installed_plugins table.
pub const PLUGIN_V001: &str = include_str!("plugin/V001__init.sql");
/// Plugin marketplace: cached verified catalogs.
pub const PLUGIN_MARKETPLACE_V001: &str = include_str!("plugin_marketplace/V001__init.sql");
/// Per-site collaborators and site_grants tables.
pub const COLLABORATORS_V001: &str = include_str!("collaborators/V001__init.sql");
/// Container registry initial schema.
pub const CONTAINER_REGISTRY_V001: &str = include_str!("container_registry/V001__init.sql");
/// AI Ops sessions, messages, and actions.
pub const AI_OPS_V001: &str = include_str!("ai_ops/V001__init.sql");
/// Compliance: hardening runs, audit retention policy, GDPR exports.
pub const COMPLIANCE_V001: &str = include_str!("compliance/V001__init.sql");
/// Service manager: lifecycle action history.
pub const SERVICE_MANAGER_V001: &str = include_str!("service_manager/V001__init.sql");
/// OS update management: policy and apply history.
pub const OS_UPDATES_V001: &str = include_str!("os_updates/V001__init.sql");
/// Synthetic monitoring: checks and per-run results.
pub const SYNTHETIC_MONITORING_V001: &str = include_str!("synthetic_monitoring/V001__init.sql");
/// Log viewer: download audit rows.
pub const LOG_VIEWER_V001: &str = include_str!("log_viewer/V001__init.sql");
/// Database privilege management: grants, remote access, SSO.
pub const DB_PRIVILEGES_V001: &str = include_str!("db_privileges/V001__init.sql");
/// IPv6 + address-pool: pools and per-site allocations.
pub const IP_ALLOCATION_V001: &str = include_str!("ip_allocation/V001__init.sql");
/// Reseller billing: meters, chargebacks, integrations.
pub const BILLING_V001: &str = include_str!("billing/V001__init.sql");
/// Load balancing and failover: pools and members.
pub const LOAD_BALANCING_V001: &str = include_str!("load_balancing/V001__init.sql");
/// WordPress toolkit: managed sites and update runs.
pub const WORDPRESS_TOOLKIT_V001: &str = include_str!("wordpress_toolkit/V001__init.sql");
/// Wildcard SSL with DNS-01: cert requests and TXT leases.
pub const WILDCARD_SSL_V001: &str = include_str!("wildcard_ssl/V001__init.sql");
/// Non-PHP runtime: per-site runtime config.
pub const APP_RUNTIMES_V001: &str = include_str!("app_runtimes/V001__init.sql");
/// Kernel resource isolation: cgroup limits + namespace + policy.
pub const KERNEL_ISOLATION_V001: &str = include_str!("kernel_isolation/V001__init.sql");
