//! Browser-facing previews: per-site tab + top-level directory.
//!
//! `page` renders the cross-site directory at `/previews`, listing
//! every site visible to the caller and the count of live previews on
//! it. `site_previews` renders the per-site page at
//! `/sites/{id}/previews` showing the actual rows with state badges,
//! TTL countdown, and the `redeploy` / `destroy` actions. All state
//! mutations POST to the JSON API endpoints; this module is pure
//! read-side rendering.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use maud::html;
use openpanel_domain::User;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

/// Top-level `/previews` directory page: every visible site with its
/// live preview count.
pub async fn page(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let rows = collect_site_rows(&state, &user).await;
    let content = html! {
        h1 { "Pull-request previews" }
        p { "Per-PR ephemeral environments built from signed webhooks. \
              Browse a site to view its preview history, rebuild, or destroy." }
        @if rows.is_empty() {
            (crate::ui_states::EmptyState::new(
                "No sites yet",
                "Create a site and link a git repository to enable PR previews."
            ).render())
        } @else {
            table class="table" {
                thead {
                    tr {
                        th { "Domain" }
                        th { "Live previews" }
                        th { "Actions" }
                    }
                }
                tbody {
                    @for row in &rows {
                        tr {
                            td { a href=(format!("/sites/{}", row.id)) { (row.domain) } }
                            td { (row.live) }
                            td class="actions" {
                                a href=(format!("/sites/{}/previews", row.id)) { "Open" }
                            }
                        }
                    }
                }
            }
        }
    };
    let path = "/previews";
    state
        .render_shell(&user, &csrf, path, content)
        .await
        .into_response()
}

/// Per-site `/sites/{id}/previews` page: live and historical
/// previews with `redeploy` and `destroy` actions.
pub async fn site_previews(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Path(site_id): Path<Uuid>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let previews = match state.previews.list(&user, site_id).await {
        Ok(rows) => rows,
        Err(error) => {
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("could not load previews: {error}"),
            )
                .into_response();
        }
    };
    let content = html! {
        h1 { "Previews" }
        a href=(format!("/sites/{}", site_id)) { "Back to site" }
        h2 { "History" }
        @if previews.is_empty() {
            (crate::ui_states::EmptyState::new(
                "No previews yet",
                "Open a pull request against a linked repository to create the first preview."
            ).render())
        } @else {
            table class="table" {
                thead {
                    tr {
                        th { "PR" }
                        th { "State" }
                        th { "Hostname" }
                        th { "Expires" }
                        th { "Reason" }
                        th { "Actions" }
                    }
                }
                tbody {
                    @for preview in &previews {
                        tr {
                            td { (preview.pr_number()) }
                            td {
                                span class={ "status status-" (preview.state().as_str()) } {
                                    (preview.state().as_str())
                                }
                            }
                            td { code { (preview.hostname()) } }
                            td { (expires_label(preview.expires_at(), Utc::now())) }
                            td { (preview.destroy_reason().unwrap_or("—")) }
                            td class="actions" {
                                @if !preview.is_terminal() {
                                    form class="inline" method="post"
                                        action={ "/api/v1/sites/" (site_id) "/previews/" (preview.pr_number()) "/redeploy" } {
                                        (crate::layout::csrf_field(&csrf))
                                        button type="submit" { "Redeploy" }
                                    }
                                    form class="inline" method="post" action={ "/api/v1/sites/" (site_id) "/previews/" (preview.pr_number()) "/delete" } {
                                        (crate::layout::csrf_field(&csrf))
                                        button type="submit" { "Destroy" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    let path = format!("/sites/{site_id}/previews");
    state
        .render_shell(&user, &csrf, &path, content)
        .await
        .into_response()
}

/// One row in the top-level previews directory.
struct SiteRow {
    id: Uuid,
    domain: String,
    live: usize,
}

async fn collect_site_rows(state: &WebState, user: &User) -> Vec<SiteRow> {
    let Ok(sites) = state.sites.list_sites(user).await else {
        return Vec::new();
    };
    let mut rows = Vec::with_capacity(sites.len());
    for site in &sites {
        let live = state
            .previews
            .list(user, site.id())
            .await
            .map(|previews| previews.iter().filter(|p| !p.is_terminal()).count())
            .unwrap_or(0);
        rows.push(SiteRow {
            id: site.id(),
            domain: site.primary_domain().to_string(),
            live,
        });
    }
    rows.sort_by(|a, b| a.domain.cmp(&b.domain));
    rows
}

fn expires_label(expires_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    match expires_at {
        Some(ts) if ts > now => format!("in {}h", (ts - now).num_hours()),
        Some(_) => "expired".into(),
        None => "—".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expires_label_for_future_is_relative_hours() {
        let now = Utc::now();
        let later = now + chrono::Duration::hours(5);
        let out = expires_label(Some(later), now);
        assert_eq!(out, "in 5h");
    }

    #[test]
    fn expires_label_for_past_is_expired() {
        let now = Utc::now();
        let earlier = now - chrono::Duration::hours(1);
        let out = expires_label(Some(earlier), now);
        assert_eq!(out, "expired");
    }

    #[test]
    fn expires_label_for_none_is_dash() {
        let now = Utc::now();
        assert_eq!(expires_label(None, now), "—");
    }
}
