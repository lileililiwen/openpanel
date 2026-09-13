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
/// Browser UI quality gate: route matrix, rendered checks, localization.
pub mod browser_ui_quality;
/// Capability navigation registry: one discoverability inventory.
pub mod capability_registry;
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
/// Host-fleet and terminal-safety building blocks (command classification,
/// session expiry, secret-safe host views).
pub mod host_fleet;
pub mod layer;
pub mod layout;
/// Named `hx-indicator` loading elements.
pub mod loading;
pub mod login;
pub mod logs;
pub mod mail;
pub mod monitoring;
/// Monitoring-fleet operator views: configurable dashboard views and
/// safe scoped fleet health.
pub mod monitoring_fleet;
/// Navigation model and icon set for the shell sidebar.
pub mod nav_model;
pub mod notifications;
/// Operator security control plane: queue, detail, preview, suppress,
/// remediate (owner/admin) over the shared lifecycle service.
pub mod operator_security;
/// Shared operational-workflow building blocks (file-action validation,
/// capacity-aware backup decisions, secret-safe rows, task-state banners).
pub mod ops_workflows;
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
/// Site-scoped landing routes (domains, runtime, logs, backups).
pub mod site_scoped;
/// Per-site staging web pages.
pub mod site_staging;
/// Site workspace: capability-filtered tabbed navigation shared by site pages.
pub mod site_workspace;
pub mod sites;
pub mod software_center;
/// Software Center trust + provenance building blocks (fail-closed digest
/// classification, aggregated trust view, compatibility + permission
/// summaries, secret-safe recovery copy).
pub mod software_center_trust;
pub mod ssl;
/// Public, unauthenticated status page (`/status/{slug}`).
pub mod status_page;
/// Admin settings page for the status page (`/status-page`).
pub mod status_page_admin;
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
