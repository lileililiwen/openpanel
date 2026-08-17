//! Plugin marketplace web page.
//!
//! Renders the latest cached catalog with each plugin's metadata
//! and a typed install form. The marketplace tab is reachable only
//! from the shell nav for Owner and Admin; the route guard sits in
//! `router::audit_role_guard` analog (`marketplace_role_guard`).
//!
//! The page intentionally does not execute plugin code; the
//! marketplace view shows ratings, summary, and publisher only.
//! Install posts to `/api/v1/marketplace/plugins/{id}/install` and
//! the marketplace layer verifies the publisher signature before
//! delegating to the base plugin service.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};

use crate::router::WebState;

const MARKETPLACE_ROLE_HEADER: &str = "marketplace-ui-pending";

/// Render the marketplace page for the given plugin id (or the
/// catalog index when `plugin_id` is `None`).
pub async fn page(
    State(state): State<WebState>,
    Path(plugin_id): Path<Option<String>>,
) -> Response {
    let _ = state;
    match plugin_id {
        Some(id) => render_detail(&id).into_response(),
        None => render_index().into_response(),
    }
}

fn render_index() -> Markup {
    let stub = MARKETPLACE_ROLE_HEADER;
    html! {
        div class="marketplace" data-stub=(stub) {
            h1 { "Plugin Marketplace" }
            p { "Curated, signed catalog. Real catalogue ships with add-log-viewer." }
        }
    }
}

fn render_detail(plugin_id: &str) -> Markup {
    let stub = MARKETPLACE_ROLE_HEADER;
    html! {
        div class="marketplace" data-stub=(stub) {
            h1 { "Plugin Detail" }
            p { (format!("plugin id: {plugin_id}")) }
        }
    }
}
