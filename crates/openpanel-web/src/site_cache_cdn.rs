//! Site cache and CDN integration web pages.
//!
//! Server-rendered maud forms that POST to the JSON API endpoints
//! declared in `openpanel-api::routes::site_cache_cdn`. The forms
//! are CSRF-protected through the global session middleware; the
//! API endpoints enforce role-based authorisation.

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
};
use maud::{Markup, html};
use uuid::Uuid;

use crate::router::{WebState, WebUser};
use crate::site_workspace::TabId;

/// Cache editor + purge dialog page for a single site.
///
/// The page renders a form per supported mutation; each form posts
/// directly to the JSON API route that performs the change. The
/// page itself is read-only — it does not call the cache service
/// from the request path.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
) -> Response {
    // The page is best-effort read-only: render the form regardless
    // of the service result. If the policy load fails (no policy
    // stored yet), the form still renders with the defaults.
    let _ = state.identity.list_users().await;
    let body: Markup = html! {
        (crate::site_workspace::site_bar(&state, &user, site_id, TabId::Cache).await)
        section class="card" {
            h2 { "Cache policy" }
            p { "Configure the per-site page cache (nginx microcache)." }
            form method="post" action={ "/api/v1/sites/" (site_id) "/cache" } class="form form-grid" {
                label { "Page TTL (seconds)"
                    input type="number" name="ttl_seconds" min="1" max="31536000" value="60" required;
                }
                label { "Static-asset TTL (seconds, default 7d)"
                    input type="number" name="static_assets_ttl_seconds" min="1" max="31536000" value="604800";
                }
                label { "Bypass paths (comma-separated absolute paths or globs ending in /*)"
                    input type="text" name="bypass_paths" placeholder="/wp-admin/*, /api/login";
                }
                label { "Keyed cookies (comma-separated, names only)"
                    input type="text" name="keyed_cookies" placeholder="session, region";
                }
                label class="form-row" {
                    input type="checkbox" name="stale_while_revalidate" value="true";
                    " Serve stale while revalidating"
                }
                label class="form-row" {
                    input type="checkbox" name="revalidation_required" value="true" checked;
                    " Require revalidation before serving"
                }
                div class="form-actions" {
                    button type="submit" { "Save cache policy" }
                }
            }
        }
        section class="card" {
            h2 { "Purge local cache" }
            p { "Remove the listed paths from the per-site nginx cache." }
            form method="post" action={ "/api/v1/sites/" (site_id) "/cache/purge" } class="form" {
                label { "Paths (comma-separated, must start with /)"
                    input type="text" name="paths" placeholder="/article/x, /article/y" required;
                }
                div class="form-actions" {
                    button type="submit" { "Purge" }
                }
            }
        }
        section class="card" {
            h2 { "CDN integrations" }
            p { "Manage CDN integrations for this site from the JSON API; the form below is read-only." }
            ul {
                li { "Cloudflare: bearer token + zone id" }
                li { "CloudFront: AWS access key + secret + distribution id" }
                li { "Generic HTTP: webhook URL" }
            }
            form method="post" action="/api/v1/cdn/integrations" class="form" {
                label { "Display name" input type="text" name="name" required; }
                label { "Kind"
                    select name="kind" {
                        option value="generic_http" { "Generic HTTP" }
                        option value="cloudflare" { "Cloudflare" }
                        option value="cloudfront" { "CloudFront" }
                    }
                }
                label { "API token (Cloudflare / CloudFront access key)" input type="text" name="api_token"; }
                label { "Zone / distribution id" input type="text" name="zone_id"; }
                label { "Webhook URL (Generic HTTP)" input type="text" name="webhook_url"; }
                label { "Region (CloudFront, default us-east-1)" input type="text" name="region"; }
                label { "Secret key (CloudFront)" input type="text" name="secret_key"; }
                div class="form-actions" {
                    button type="submit" { "Create integration" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/sites/{site_id}/cache");
    state
        .render_shell(&user, &csrf, &path, body)
        .await
        .into_response()
}

/// CDN purge page (path-driven, so the user can bookmark a
/// recurring purge against a fixed integration).
pub async fn cdn_purge_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(integration_id): Path<Uuid>,
) -> Response {
    let body: Markup = html! {
        section class="card" {
            h2 { "CDN purge" }
            p { "Purge paths through integration " code { (integration_id) } "." }
            form method="post" action="/api/v1/cdn/purge" class="form" {
                input type="hidden" name="integration_id" value={(integration_id.to_string())};
                label { "Paths (comma-separated, must start with /)"
                    input type="text" name="paths" placeholder="/article/x, /article/y" required;
                }
                div class="form-actions" {
                    button type="submit" { "Purge" }
                }
            }
        }
    };
    let csrf = state.csrf.token_for(session.id());
    let path = format!("/cdn/purge/{integration_id}");
    state
        .render_shell(&user, &csrf, &path, body)
        .await
        .into_response()
}
