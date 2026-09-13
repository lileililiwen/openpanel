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
mod backups;
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
mod dns;
mod docker;
mod files;
mod ftp;
/// Admin SSH host-key integration tests.
mod host_ssh_keys;
/// Hosting plans HTTP integration tests.
mod hosting_plans;
mod identity;
/// Log rotation-policy integration tests.
mod log_policies;
mod logs;
mod mail;
/// Malware scanner integration tests.
/// Mail end-user surface integration tests.
mod mail_surfaces;
mod malware_scanner;
/// Migration importers preview / run / rollback integration tests.
mod migration_importers;
mod monitoring;
/// Offsite backup targets credential / remote-config integration tests.
mod offsite_backup_targets;
/// Plugin extension framework integration tests.
mod plugin_extension;
/// Plugin marketplace HTTP integration tests.
mod plugin_marketplace;
mod quality;
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
mod sso;
mod system_services;
/// Themeable UI / white-label integration tests.
mod themeable_ui;
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
mod webmail_client;
