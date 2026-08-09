//! Web adapter: server-rendered HTML UI (HTMX) for the control panel.
//!
//! This crate hosts the browser-facing shell: login/logout, the HTMX-driven
//! layout (sidebar + topbar + content region), static assets, and per-session
//! CSRF protection. It reuses the API's session middleware and
//! `openpanel_session` cookie so there is exactly one auth system.

#![deny(rustdoc::broken_intra_doc_links)]
// Tests MAY use unwrap/expect/panic freely (same convention as other crates).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod assets;
pub mod csrf;
pub mod dashboard;
pub mod databases;
pub mod files;
pub mod layout;
pub mod login;
pub mod monitoring;
pub mod router;
pub mod settings;
pub mod sites;
pub mod ssl;
pub mod users;

pub use router::{WebRuntime, WebState, router};
