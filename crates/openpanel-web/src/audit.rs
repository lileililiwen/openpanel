//! Audit & activity center: owner-only HTML page, HTMX fragment, and
//! JSON API over the append-only audit log.
//!
//! Every response is rendered from a redacted projection
//! (`openpanel_core::AuditView`), so secrets, tokens, and request
//! bodies never reach the browser or the JSON surface. Reads are
//! read-only: viewing the audit log never appends an audit event.

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
};
use chrono::{DateTime, Utc};
use maud::{Markup, html};
use openpanel_core::audit::{AuditCursor, AuditPage, AuditQuery, AuditView};
use openpanel_domain::Role;
use serde::Deserialize;

use crate::{
    router::{WebState, WebUser},
    ui_states::NoResultsState,
};

/// Forbidden response used by the role guard. Symmetric with the rest
/// of the web adapter which returns a plain `403` for unauthorized
/// requests.
pub fn forbidden() -> Response {
    let mut resp = (StatusCode::FORBIDDEN, "forbidden").into_response();
    resp.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp
}

/// Validate the role guard. Owner and Admin are allowed; User is not.
pub fn role_guard(role: Role) -> bool {
    matches!(role, Role::Owner | Role::Admin)
}

/// Raw GET query parameters accepted by the audit routes.
#[derive(Debug, Default, Deserialize)]
pub struct AuditQueryParams {
    /// Filter by exact actor identifier.
    actor: Option<String>,
    /// Filter by exact action value (snake_case string).
    action: Option<String>,
    /// Substring filter on the target.
    target: Option<String>,
    /// Filter by exact outcome (`success` / `failure` / `denied`).
    outcome: Option<String>,
    /// Inclusive lower time bound (RFC3339).
    from: Option<String>,
    /// Inclusive upper time bound (RFC3339).
    to: Option<String>,
    /// Pagination cursor from a previous page.
    cursor: Option<String>,
    /// Max events per page (clamped by the service).
    limit: Option<usize>,
}

impl AuditQueryParams {
    /// Build a typed [`AuditQuery`], dropping unparseable time bounds.
    fn to_query(&self) -> AuditQuery {
        let from = self.from.as_deref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        });
        let to = self.to.as_deref().and_then(|s| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        });
        let cursor = self.cursor.as_deref().and_then(AuditCursor::decode);
        AuditQuery {
            actor: self.actor.clone(),
            action: self.action.clone(),
            target: self.target.clone(),
            outcome: self.outcome.clone(),
            from,
            to,
            cursor,
            limit: self.limit.unwrap_or(50),
        }
    }

    /// Stable, human-readable description of the active filters for the
    /// no-results state.
    #[allow(dead_code)]
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if let Some(a) = &self.actor {
            parts.push(format!("actor={a}"));
        }
        if let Some(a) = &self.action {
            parts.push(format!("action={a}"));
        }
        if let Some(t) = &self.target {
            parts.push(format!("target~{t}"));
        }
        if let Some(o) = &self.outcome {
            parts.push(format!("outcome={o}"));
        }
        if !parts.is_empty() {
            parts.join(" ")
        } else {
            "(no filters)".to_string()
        }
    }
}

/// Minimal percent-encoding for query-string values (safe subset).
fn url_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => out.push(c),
            _ => {
                for b in c.to_string().bytes() {
                    out.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    out
}

/// GET /audit — the full audit & activity page inside the shell.
pub async fn audit_index(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Query(params): Query<AuditQueryParams>,
) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let page = match state.audit.query(params.to_query()).await {
        Ok(page) => page,
        Err(_) => {
            let content = html! {
                h1 { "Audit & activity" }
                section class="op-error-state" aria-live="assertive" role="alert" {
                    h2 { "Unable to load audit log" }
                    p { "The audit service is temporarily unavailable. Please retry." }
                    a class="btn" hx-get="/audit/events" hx-target="#content" { "Retry" }
                }
            };
            return state
                .render_shell(&user, &csrf, "/audit", content)
                .await
                .into_response();
        }
    };
    let content = render_page(&page);
    state
        .render_shell(&user, &csrf, "/audit", content)
        .await
        .into_response()
}

/// GET /audit/events — the HTMX fragment or JSON page.
///
/// Returns an `application/json` page when the request is not an HTMX
/// request (e.g. API callers), and the HTML `<section>` fragment for
/// in-page swaps otherwise.
pub async fn audit_list(
    State(state): State<WebState>,
    WebUser(_user, _): WebUser,
    headers: HeaderMap,
    Query(params): Query<AuditQueryParams>,
) -> Response {
    let page = match state.audit.query(params.to_query()).await {
        Ok(page) => page,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Unable to load audit events",
            )
                .into_response();
        }
    };
    if headers.get("HX-Request").is_some() {
        render_events_fragment(&page).into_response()
    } else {
        Json(page).into_response()
    }
}

/// Render the full page content: summary, filters, then the table or a
/// terminal UI state.
fn render_page(page: &AuditPage) -> Markup {
    html! {
        h1 { "Audit & activity" }
        p class="audit-intro" {
            "Every owner and admin action is recorded. Reads are not audited. "
            "Secrets, tokens, and request bodies are never shown."
        }
        (render_summary(page))
        (render_filters())
        (render_events_region(page))
    }
}

/// Compact summary: failures in view, total in view, and a freshness note.
fn render_summary(page: &AuditPage) -> Markup {
    let failures = page
        .events
        .iter()
        .filter(|e| e.outcome == "failure")
        .count();
    let total = page.events.len();
    html! {
        section id="audit-summary" class="audit-summary" aria-live="polite" {
            div class="audit-stat" {
                span class="audit-stat-value" { (total) }
                span class="audit-stat-label" { "shown" }
            }
            div class="audit-stat" {
                span class="audit-stat-value" { (failures) }
                span class="audit-stat-label" { "failures" }
            }
            @if let Some(newest) = page.events.first() {
                div class="audit-stat" {
                    span class="audit-stat-label" { "latest" }
                    time datetime=(newest.ts.to_rfc3339()) { (newest.ts.to_rfc3339()) }
                }
            }
        }
    }
}

/// The filter bar (GET form) plus the swap target for the event table.
fn render_filters() -> Markup {
    html! {
        form class="form form-inline" method="get" action="/audit" {
            label { "Actor" input type="text" name="actor" autocomplete="off"; }
            label { "Action"
                input type="text" name="action" placeholder="e.g. site_created" autocomplete="off";
            }
            label { "Target"
                input type="text" name="target" placeholder="substring" autocomplete="off";
            }
            label { "Outcome"
                select name="outcome" {
                    option value="" { "Any outcome" }
                    option value="success" { "Success" }
                    option value="failure" { "Failure" }
                    option value="denied" { "Denied" }
                }
            }
            label { "From" input type="datetime-local" name="from"; }
            label { "To" input type="datetime-local" name="to"; }
            button type="submit" { "Filter" }
            a class="btn btn-ghost" href="/audit" { "Clear" }
        }
    }
}

/// Decide between the populated table, the empty state, and the
/// no-results state.
fn render_events_region(page: &AuditPage) -> Markup {
    if !page.events.is_empty() {
        return render_events_fragment(page);
    }
    // The index handler always loads without filters, so a populated
    // log that matches nothing is represented by the no-results state,
    // while a completely empty log uses the empty state.
    NoResultsState::new("(no filters)")
        .with_clear_href("/audit")
        .render()
}

/// The swap target: an HTMX-refreshed table of audit events.
fn render_events_fragment(page: &AuditPage) -> Markup {
    html! {
        section id="audit-events" class="audit-events"
            hx-get="/audit/events"
            hx-trigger="every 30s"
            hx-swap="outerHTML"
            hx-indicator="#audit-loading"
            aria-live="polite" {
            div id="audit-loading" class="htmx-indicator" { (crate::ui_states::LoadingState::new("Loading activity…").render()) }
            @if page.events.is_empty() {
                p class="audit-empty-note" { "No events match the current filters." }
            } @else {
                table class="audit-table" {
                    thead {
                        tr {
                            th { "Time" }
                            th { "Actor" }
                            th { "Action" }
                            th { "Target" }
                            th { "Outcome" }
                            th { "Details" }
                        }
                    }
                    tbody {
                        @for view in &page.events {
                            (render_event_row(view))
                        }
                    }
                }
            }
            @if let Some(cursor) = &page.next_cursor {
                a class="btn audit-older" href=(format!("/audit?cursor={}", url_escape(&cursor.encode()))) {
                    "Older activity"
                }
            }
        }
    }
}

/// One audit event row with a `<details>` disclosure of safe metadata.
fn render_event_row(view: &AuditView) -> Markup {
    // Bound once so the template below can match on it instead of
    // re-opening the object and unwrapping a value that only the
    // surrounding `@if` guarantees.
    let meta = view.metadata.as_object().filter(|m| !m.is_empty());
    html! {
        tr class=(format!("audit-row audit-outcome-{}", view.outcome)) {
            td { time datetime=(view.ts.to_rfc3339()) { (view.ts.to_rfc3339()) } }
            td { (view.actor) }
            td { code { (view.action) } }
            td { (view.target.as_deref().unwrap_or("—")) }
            td { span class=(format!("audit-badge audit-badge-{}", view.outcome)) { (view.outcome) } }
            td {
                @if let Some(meta) = meta {
                    details class="audit-meta" {
                        summary { "metadata" }
                        @for (k, v) in meta {
                            div class="audit-meta-row" {
                                span class="audit-meta-key" { (k) }
                                span class="audit-meta-val" { (v.to_string()) }
                            }
                        }
                    }
                } @else {
                    span class="audit-meta-none" { "—" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use openpanel_core::audit::{AuditOutcome, AuditView};

    use super::*;

    fn view(outcome: &str, action: &str, meta: serde_json::Value) -> AuditView {
        AuditView {
            ts: Utc::now(),
            actor: "admin".into(),
            action: action.into(),
            target: Some("site.example".into()),
            source_ip: None,
            outcome: outcome.into(),
            metadata: meta,
        }
    }

    #[test]
    fn role_guard_admits_owner_and_admin_but_not_user() {
        assert!(role_guard(Role::Owner));
        assert!(role_guard(Role::Admin));
        assert!(!role_guard(Role::User));
    }

    #[test]
    fn forbidden_response_is_403() {
        let resp = forbidden();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn summary_counts_failures_and_total() {
        let page = AuditPage {
            events: vec![
                view("success", "login", serde_json::json!({})),
                view("failure", "backup_run", serde_json::json!({})),
            ],
            next_cursor: None,
        };
        let out = render_summary(&page).into_string();
        assert!(out.contains("2"), "total count: {out}");
        assert!(out.contains("1"), "failure count: {out}");
    }

    #[test]
    fn empty_events_render_no_results() {
        let page = AuditPage {
            events: vec![],
            next_cursor: None,
        };
        let out = render_events_region(&page).into_string();
        assert!(out.contains("op-no-results"), "no-results state: {out}");
    }

    #[test]
    fn event_row_hides_metadata_when_absent() {
        let v = view("success", "login", serde_json::json!(null));
        let out = render_event_row(&v).into_string();
        assert!(out.contains("audit-row"), "row: {out}");
        assert!(!out.contains("<details"), "no details when no meta: {out}");
    }

    #[test]
    fn event_row_renders_metadata_disclosure() {
        let v = view("success", "login", serde_json::json!({"plan": "pro"}));
        let out = render_event_row(&v).into_string();
        assert!(out.contains("<details"), "details when meta present: {out}");
        assert!(out.contains("plan"), "key shown: {out}");
    }

    #[test]
    fn query_params_describe_active_filters() {
        let params = AuditQueryParams {
            actor: Some("admin".into()),
            outcome: Some("failure".into()),
            ..Default::default()
        };
        assert!(params.describe().contains("actor=admin"));
        assert!(params.describe().contains("outcome=failure"));
    }

    #[test]
    fn query_params_drop_unparseable_time_bounds() {
        let params = AuditQueryParams {
            from: Some("not-a-date".into()),
            to: Some("2026-08-29T00:00:00Z".into()),
            ..Default::default()
        };
        let q = params.to_query();
        assert!(q.from.is_none());
        assert!(q.to.is_some());
        let _ = AuditOutcome::Success;
    }

    #[test]
    fn query_string_escapes_values() {
        assert_eq!(url_escape("a b&c"), "a%20b%26c");
        assert_eq!(url_escape("plain"), "plain");
    }
}
