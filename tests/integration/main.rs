// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production code. Integration tests are test code and MAY contain
// them, so we allow them here.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

#[path = "../common/mod.rs"]
mod common;

/// Account hierarchy HTTP integration tests.
mod account_hierarchy;
/// Interaction surface integration tests (layer, forms, ui states, feedback).
mod admin_interaction_surface;
mod api_tokens;
/// Application delivery bounded-context integration tests:
/// immutable release, isolated environments, health-gated
/// promotion, audited rollback.
mod application_delivery;
mod backups;
/// Browser UI quality (axe/WCAG, responsive, localization) integration tests.
mod browser_ui_quality;
/// Capability navigation + site workspace completion integration tests.
mod capability_navigation;
/// Per-site collaborator HTTP integration tests.
mod collaborators;
/// Container registry HTTP integration tests.
mod container_registry;
/// Container runtime HTTP + web integration tests.
mod container_runtime;
mod cron;
mod databases;
/// Database point-in-time recovery HTTP integration tests.
mod db_pitr;
/// Database remote-access integration tests.
mod db_remote_access;
/// Deployment-adapter contract integration tests (conformance
/// fixture, idempotency replay, dry-run, audit redaction).
mod deployment_adapters;
mod dns;
mod docker;
mod files;
/// Forge publish provider facade integration tests: capabilities,
/// preflight, publish, verify, rollback, idempotency, redaction,
/// and independent lifecycle.
mod forge_publish_provider;
mod ftp;
/// Global FTP aggregate + database row-action integration tests.
mod ftp_global;
/// Host operations and migration interoperability integration
/// tests: host capability contract, capability mismatch blocks
/// operation with remediation.
mod host_ops;
/// Admin SSH host-key integration tests.
mod host_ssh_keys;
/// Hosting plans HTTP integration tests.
mod hosting_plans;
mod identity;
/// Log rotation-policy integration tests.
mod log_policies;
mod logs;
mod mail;
/// Staging mail-capture inbox API and HTML surface tests.
mod mail_capture;
/// Malware scanner integration tests.
/// Mail end-user surface integration tests.
mod mail_surfaces;
mod malware_scanner;
/// Migration wizard web UI (`/migrate`) integration tests.
mod migrate_wizard;
/// Migration importers preview / run / rollback integration tests.
mod migration_importers;
mod monitoring;
/// Monitoring-fleet operations integration tests.
mod monitoring_fleet;
/// Offsite backup targets credential / remote-config integration tests.
mod offsite_backup_targets;
/// Operator security control-plane integration tests.
mod operator_security;
/// Plugin extension framework integration tests.
mod plugin_extension;
/// Plugin marketplace HTTP integration tests.
mod plugin_marketplace;
mod portable_runtime;
mod quality;
/// Release-evidence contract integration tests.
mod release_evidence;
/// Runtime environment integration tests.
mod runtime_env;
mod security;
mod server_snapshots;
/// Site cache and CDN policy / integration / purge integration tests.
mod site_cache_cdn;
/// Site clone + template export integration tests.
mod site_clone_template;
/// Per-site HTTP-controls REST integration tests.
mod site_http_controls;
/// Per-site staging HTTP integration tests.
mod site_staging;
/// Per-site transport tuning REST integration tests.
mod site_transport;
mod sites;
mod smoke;
mod software_center;
mod ssl;
/// ACME production-lifecycle integration tests.
mod ssl_production_lifecycle;
mod sso;
mod system_services;
/// Terminal navigation + saved servers/commands integration tests.
mod terminal_nav;
/// Themeable UI / white-label integration tests.
mod themeable_ui;
/// Verified-service-catalog integration tests: signed manifests,
/// reviewable plans, bounded-retry lifecycle jobs, and trust
/// revocation (capability `verified-service-catalog`).
mod verified_service_catalog;
mod waf;
/// Web application installer integration tests.
mod web_application_installer;
/// Webmail client integration tests.
/// Browser terminal integration tests.
mod web_terminal;
mod web_ui;
/// Web-UI audit route group integration tests.
mod web_ui_audit;
/// Web-UI styling + responsive layout integration tests.
mod web_ui_styling;
/// Per-site WebDAV account API, `/dav` mount, and HTML surface tests.
mod webdav;
mod webmail_client;
