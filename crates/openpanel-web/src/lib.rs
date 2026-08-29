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
/// Feedback widget: NPS-style rating persisted to the feedback context.
pub mod feedback;
pub mod files;
pub mod forms;
pub mod ftp;
pub mod layer;
pub mod layout;
/// Named `hx-indicator` loading elements.
pub mod loading;
pub mod login;
pub mod logs;
pub mod mail;
pub mod monitoring;
/// Navigation model and icon set for the shell sidebar.
pub mod nav_model;
/// Shared operational-workflow building blocks (file-action validation,
/// capacity-aware backup decisions, secret-safe rows, task-state banners).
pub mod ops_workflows;
pub mod notifications;
/// Plugin extension framework web page.
pub mod plugin_extension;
/// Plugin marketplace web page.
pub mod plugin_marketplace;
/// PR preview deployments web pages.
pub mod previews;
pub mod router;
pub mod security;
pub mod settings;
/// Per-site page cache and CDN integration web pages.
pub mod site_cache_cdn;
/// Per-site WAF editor page.
pub mod site_http_controls;
/// Per-site staging web pages.
pub mod site_staging;
/// Site workspace: capability-filtered tabbed navigation shared by site pages.
pub mod site_workspace;
pub mod sites;
/// Public, unauthenticated status page (`/status/{slug}`).
pub mod status_page;
/// Admin settings page for the status page (`/status-page`).
pub mod status_page_admin;
pub mod software_center;
pub mod ssl;
pub mod system_services;
/// String-table stub used by every template.
pub mod t;
/// Themeable UI / white-label editor page.
pub mod themeable_ui;
pub mod two_factor;
/// Reusable empty / no-results / loading / error state components.
pub mod ui_states;
pub mod users;
pub mod waf;
/// Static-asset contract assertions for the web UI styling
/// OpenSpec change (token-only styling, mobile-first layout,
/// focus ring, etc.).
pub mod web_ui_styling;
/// Webmail client web pages.
pub mod webmail;

pub use router::{WebRuntime, WebState, public_router, router};
