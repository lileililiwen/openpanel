//! Admin settings page for the public status page.
//!
//! Operators enable/disable the page, rotate the slug, and add or
//! remove published checks. Renders at `/status-page` (Owner/Admin
//! only) and posts back to itself.

use axum::{
    Form, Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use maud::html;
use openpanel_domain::synthetic_monitoring::Slug;
use serde::Deserialize;
use uuid::Uuid;

use crate::router::{WebState, WebUser};

#[derive(Debug, Deserialize, Default)]
pub struct EmptyForm;

#[derive(Debug, Deserialize)]
pub struct PublishForm {
    /// Check id to publish.
    pub check_id: Uuid,
    /// Label shown on the public page.
    pub label: String,
}

#[derive(Debug, Deserialize)]
pub struct UnpublishForm {
    /// Check id to remove from the page.
    pub check_id: Uuid,
}

/// Render the admin settings page.
pub async fn page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let page = match state.status_page.get().await {
        Ok(page) => page,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("could not load status page: {error}"),
            )
                .into_response();
        }
    };
    let all_checks = state
        .status_page
        .list_available_checks()
        .await
        .unwrap_or_default();
    let published: std::collections::HashSet<Uuid> =
        page.entries.iter().map(|e| e.check_id).collect();
    let available_checks: Vec<_> = all_checks
        .into_iter()
        .filter(|c| !published.contains(&c.id))
        .collect();
    let content = html! {
        h1 { "Status page" }
        p class="intro" {
            "Publish a public, anonymous view of your synthetic monitoring checks. \
             Visitors get a short-lived cacheable URL — only the labels you set are exposed."
        }
        section class="status-page-policy" {
            h2 { "Policy" }
            dl class="kv" {
                dt { "Enabled" } dd { (if page.enabled { "yes" } else { "no" }) }
                dt { "Public slug" } dd { code { (page.slug.as_str()) } }
                dt { "Public URL" } dd { code { (format!("/status/{}", page.slug.as_str())) } }
                dt { "Published entries" } dd { (page.entries.len()) }
            }
            form method="post" action="/status-page/enable" class="inline" {
                input type="hidden" name="csrf" value=(csrf);
                button type="submit" { "Enable" }
            }
            form method="post" action="/status-page/disable" class="inline" {
                input type="hidden" name="csrf" value=(csrf);
                button type="submit" { "Disable" }
            }
            form method="post" action="/status-page/regenerate-slug" class="inline" {
                input type="hidden" name="csrf" value=(csrf);
                button type="submit" { "Regenerate slug" }
            }
        }
        section class="status-page-entries" {
            h2 { "Published checks" }
            @if page.entries.is_empty() {
                p { "No checks published yet." }
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Label" }
                            th { "Check id" }
                            th { "Actions" }
                        }
                    }
                    tbody {
                        @for entry in &page.entries {
                            tr {
                                td { (entry.label) }
                                td { code { (entry.check_id.to_string()) } }
                                td class="actions" {
                                    form method="post" action="/status-page/unpublish" class="inline" {
                                        input type="hidden" name="csrf" value=(csrf);
                                        input type="hidden" name="check_id" value=(entry.check_id.to_string());
                                        button type="submit" { "Unpublish" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        section class="status-page-publish" {
            h2 { "Publish a check" }
            @if available_checks.is_empty() {
                p { "All registered checks are already published." }
            } @else {
                form method="post" action="/status-page/publish" {
                    input type="hidden" name="csrf" value=(csrf);
                    label for="check_id" { "Check" }
                    select name="check_id" id="check_id" {
                        @for check in &available_checks {
                            option value=(check.id.to_string()) { (check.name) }
                        }
                    }
                    label for="label" { "Label" }
                    input type="text" name="label" id="label" required="true" placeholder="API";
                    button type="submit" { "Publish" }
                }
            }
        }
    };
    let path = "/status-page";
    state
        .render_shell(&user, &csrf, path, content)
        .await
        .into_response()
}

/// Enable the page.
pub async fn enable(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    Form(_form): Form<EmptyForm>,
) -> Response {
    let _ = state.status_page.enable(&user).await;
    redirect()
}

/// Disable the page.
pub async fn disable(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    Form(_form): Form<EmptyForm>,
) -> Response {
    let _ = state.status_page.disable(&user).await;
    redirect()
}

/// Rotate the slug.
pub async fn regenerate_slug(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    Form(_form): Form<EmptyForm>,
) -> Response {
    let _ = state.status_page.regenerate_slug(&user).await;
    redirect()
}

/// Publish a check.
pub async fn publish(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    Form(form): Form<PublishForm>,
) -> Response {
    let _ = state.status_page.publish(&user, form.check_id, form.label).await;
    redirect()
}

/// Unpublish a check.
pub async fn unpublish(
    State(state): State<WebState>,
    WebUser(user, _session): WebUser,
    Form(form): Form<UnpublishForm>,
) -> Response {
    let _ = state.status_page.unpublish(&user, form.check_id).await;
    redirect()
}

fn redirect() -> Response {
    use axum::http::header::LOCATION;
    (
        StatusCode::SEE_OTHER,
        [(LOCATION, "/status-page")],
        "",
    )
        .into_response()
}

#[doc(hidden)]
pub fn _unused() -> Option<Slug> {
    None
}

#[doc(hidden)]
pub fn _unused_json() -> Json<()> {
    Json(())
}
