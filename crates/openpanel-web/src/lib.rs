//! Web adapter: server-rendered HTML UI (HTMX) for the control panel.
//!
//! This crate hosts the browser-facing shell: login/logout, the HTMX-driven
//! layout (sidebar + topbar + content region), static assets, and per-session
//! CSRF protection. It reuses the API's session middleware and
//! `openpanel_session` cookie so there is exactly one auth system.

#![deny(rustdoc::broken_intra_doc_links)]
// Tests MAY use unwrap/expect/panic freely (same convention as other crates).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod api_tokens;
pub mod assets;
/// Audit log route stubs.
pub mod audit;
pub mod backups;
/// Per-site collaborator web page.
pub mod collaborators;
/// Container registry web page.
pub mod container_registry;
/// Container runtime web pages (quota editor + registry credentials).
pub mod container_runtime;
pub mod cron;
pub mod csrf;
pub mod dashboard;
pub mod databases;
/// Database point-in-time recovery web pages.
pub mod db_pitr;
pub mod dns;
pub mod docker;
pub mod files;
pub mod ftp;
pub mod layout;
pub mod login;
pub mod logs;
pub mod mail;
pub mod monitoring;
pub mod notifications;
/// Plugin marketplace web page.
pub mod plugin_marketplace;
pub mod router;
pub mod security;
pub mod settings;
/// Per-site page cache and CDN integration web pages.
pub mod site_cache_cdn;
/// Per-site staging web pages.
pub mod site_staging;
pub mod sites;
pub mod software_center;
pub mod ssl;
pub mod system_services;
/// String-table stub used by every template.
pub mod t;
pub mod two_factor;
pub mod users;
/// Per-site WAF editor page.
pub mod waf;
/// Static-asset contract assertions for the web UI styling
/// OpenSpec change (token-only styling, mobile-first layout,
/// focus ring, etc.).
pub mod web_ui_styling;

pub use router::{WebRuntime, WebState, router};
