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
mod dns;
mod docker;
mod files;
mod ftp;
/// Hosting plans HTTP integration tests.
mod hosting_plans;
mod identity;
mod logs;
mod mail;
/// Malware scanner integration tests.
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
mod security;
/// Site cache and CDN policy / integration / purge integration tests.
mod site_cache_cdn;
/// Site clone + template export integration tests.
mod site_clone_template;
/// Per-site staging HTTP integration tests.
mod site_staging;
mod sites;
mod smoke;
mod software_center;
mod ssl;
mod system_services;
/// Themeable UI / white-label integration tests.
mod themeable_ui;
mod waf;
/// Web application installer integration tests.
mod web_application_installer;
mod web_ui;
/// Web-UI audit route group integration tests.
mod web_ui_audit;
/// Web-UI styling + responsive layout integration tests.
mod web_ui_styling;
/// Webmail client integration tests.
mod webmail_client;
