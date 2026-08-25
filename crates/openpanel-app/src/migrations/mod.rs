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
/// SQL for the identity v005 migration (adds `parent_account_id` and
/// `hosting_plan_id` columns to `users` plus indexes for the
/// `add-account-hierarchy` and `add-hosting-plans` follow-on changes).
pub const IDENTITY_V005: &str = include_str!("identity/V005__hierarchy_and_plan.sql");
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
/// Terminal tickets and session records.
pub const WEB_TERMINAL_V001: &str = include_str!("web_terminal/V001__init.sql");
/// Per-site HTTP controls documents.
pub const SITE_HTTP_CONTROLS_V001: &str = include_str!("site_http_controls/V001__init.sql");
/// Per-site WAF rules and hit metrics.
pub const WAF_V001: &str = include_str!("waf/V001__init.sql");
/// Docker desired state, stacks, and trusted image patterns.
pub const DOCKER_V001: &str = include_str!("docker/V001__init.sql");
/// Per-site FTP accounts.
pub const FTP_V001: &str = include_str!("ftp/V001__init.sql");
/// Scoped personal access tokens.
pub const API_TOKENS_V001: &str = include_str!("api_tokens/V001__init.sql");
/// Feedback widget submissions (`feedback` table).
pub const FEEDBACK_V001: &str = include_str!("feedback/V001__init.sql");
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
/// DNSSEC + secondary DNS: policies, keys, secondaries, glue, DS.
pub const DNSSEC_SECONDARY_V001: &str = include_str!("dnssec_secondary/V001__init.sql");
/// Mail anti-spam and filtering: policies, greylist, sieve, etc.
pub const MAIL_FILTERING_V001: &str = include_str!("mail_filtering/V001__init.sql");
/// Git deployment: repos and deploy runs.
pub const GIT_DEPLOYMENT_V001: &str = include_str!("git_deployment/V001__init.sql");
/// Scheduled maintenance windows: windows + overrides.
pub const MAINTENANCE_WINDOWS_V001: &str = include_str!("maintenance_windows/V001__init.sql");
/// Container runtime: per-user quota, registry credentials,
/// metrics samples, and monthly egress accounts.
pub const CONTAINER_RUNTIME_V001: &str = include_str!("container_runtime/V001__init.sql");
/// Hosting plans v001: `hosting_plans` table and the
/// append-only `user_plan_assignments` table.
pub const HOSTING_PLANS_V001: &str = include_str!("hosting_plans/V001__init.sql");
/// Account hierarchy v001: `account_relationships`, `quota_pools`,
/// and `pool_claims` tables.
pub const ACCOUNT_HIERARCHY_V001: &str = include_str!("account_hierarchy/V001__init.sql");
/// Quotas v001: `quota_policies` and `quota_usages` tables.
pub const QUOTAS_V001: &str = include_str!("quotas/V001__init.sql");
/// Agent v001: `agents`, `fleet_tokens`, and `recipe_manifests`
/// tables.
pub const AGENT_V001: &str = include_str!("agent/V001__init.sql");
/// Cluster data model v001: `cluster_nodes`, `shared_storage`,
/// and `replicated_databases` tables.
pub const CLUSTER_DATA_MODEL_V001: &str = include_str!("cluster_data_model/V001__init.sql");
/// Migration importers v001: `migration_runs`,
/// `imported_resources`, and `translation_log_entries` tables.
pub const MIGRATION_IMPORTERS_V001: &str = include_str!("migration_importers/V001__init.sql");
/// Offsite backup targets v001: `backup_credentials`,
/// `backup_remote_targets`, and `backup_kek_wrappers` tables.
pub const OFFSITE_BACKUP_TARGETS_V001: &str = include_str!("offsite_backup_targets/V001__init.sql");
/// Site clone + template export v001: `site_templates`,
/// `clone_plans`, `clone_runs`, and `anonymisation_tokens` tables.
pub const SITE_CLONE_TEMPLATE_V001: &str = include_str!("site_clone_template/V001__init.sql");
/// Themeable UI and white-label v001: `theme_overrides` table.
pub const THEMEABLE_UI_V001: &str = include_str!("themeable_ui/V001__init.sql");
/// Web application malware scanner v001: `scan_profiles`,
/// `scan_runs`, `scan_findings`, `quarantine_records`, and
/// `integrity_baselines` tables.
pub const MALWARE_SCANNER_V001: &str = include_str!("malware_scanner/V001__init.sql");
/// Webmail client v001: `webmail_session_tokens` table.
pub const WEBMAIL_CLIENT_V001: &str = include_str!("webmail_client/V001__init.sql");
/// Web application installer v001: `web_app_installs`,
/// `web_app_runs`, `web_app_idempotency`, and `web_app_plans`.
pub const WEB_APPLICATION_INSTALLER_V001: &str =
    include_str!("web_application_installer/V001__init.sql");
/// Site cache and CDN integration v001: `site_cache_policies`,
/// `cdn_integrations`, and `cdn_purge_log` tables.
pub const SITE_CACHE_CDN_V001: &str = include_str!("site_cache_cdn/V001__init.sql");
