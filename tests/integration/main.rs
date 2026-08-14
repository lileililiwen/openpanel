// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production code. Integration tests are test code and MAY contain
// them, so we allow them here.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

#[path = "../common/mod.rs"]
mod common;

mod api_tokens;
mod backups;
mod cron;
mod databases;
/// Database point-in-time recovery HTTP integration tests.
mod db_pitr;
mod dns;
mod docker;
mod files;
mod ftp;
mod identity;
mod logs;
mod mail;
mod monitoring;
mod quality;
mod security;
/// Per-site staging HTTP integration tests.
mod site_staging;
mod sites;
mod smoke;
/// Plugin marketplace HTTP integration tests.
mod plugin_marketplace;
mod software_center;
mod ssl;
mod system_services;
mod waf;
mod web_ui;
/// Web-UI audit route group integration tests.
mod web_ui_audit;
