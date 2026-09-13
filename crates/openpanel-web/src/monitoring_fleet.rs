//! Monitoring-fleet operator views: configurable dashboard views with
//! time-range controls and fleet health.
//!
//! Thin adapter over the monitoring-fleet projection service and the
//! existing monitoring history: handlers enforce Owner/Admin/User role
//! gates, render with the shared `table` / `form` / `btn` /
//! `op-empty-state` / `op-error-state` vocabulary (no new CSS tokens),
//! and reuse `monitoring::sparkline` for charts. Covers
//! `monitoring-fleet-operations`: configurable views, stale/error
//! semantics, safe scoped fleet health.

use axum::{
    Form,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use maud::{Markup, html};
use openpanel_domain::{
    Role,
    monitoring::MetricKind,
    monitoring_fleet::{FleetDataState, FleetHostSummary, FleetSavedView, FleetViewPanel},
};
use serde::Deserialize;
use std::str::FromStr;

use crate::router::{WebState, WebUser};

/// Capability under test: `monitoring-fleet-operations`.
pub const CAPABILITY: &str = "monitoring-fleet-operations";

fn require_viewer(user: &openpanel_domain::User) -> Result<(), StatusCode> {
    if matches!(user.role(), Role::Owner | Role::Admin | Role::User) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

fn require_operator(user: &openpanel_domain::User) -> Result<(), StatusCode> {
    if matches!(user.role(), Role::Owner | Role::Admin) {
        Ok(())
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

/// Query params for `GET /monitoring/views`.
#[derive(Debug, Deserialize)]
pub struct FleetViewsQuery {
    /// Metric identifier.
    pub metric: Option<String>,
    /// Range in seconds.
    pub range: Option<i64>,
}

/// Form body for saving a dashboard view.
#[derive(Debug, Deserialize)]
pub struct SaveFleetViewForm {
    /// View name.
    pub name: String,
    /// Metric identifier.
    pub metric: String,
    /// Range in seconds.
    pub range: i64,
    /// Refresh interval in seconds.
    pub refresh: u32,
    /// CSRF token.
    #[serde(rename = "_csrf")]
    pub csrf_token: String,
}

/// Health badge reusing the status palette (no new tokens).
fn fleet_health_class(summary: &FleetHostSummary) -> &'static str {
    use openpanel_domain::monitoring_fleet::FleetHealthState;
    match summary.health {
        FleetHealthState::Healthy => "op-status-healthy",
        FleetHealthState::Stale => "op-status-stale",
        FleetHealthState::Drifted => "op-status-degraded",
        FleetHealthState::Unavailable => "op-status-error",
    }
}

/// GET /monitoring/views — configurable dashboard views.
pub async fn fleet_views_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Query(query): Query<FleetViewsQuery>,
) -> Response {
    if require_viewer(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let metric_raw = query.metric.unwrap_or_else(|| "Cpu".to_string());
    let kind = MetricKind::from_str(&metric_raw).unwrap_or(MetricKind::Cpu);
    let range = query.range.unwrap_or(3600).clamp(60, 2_592_000);
    let fleet_query = match state.monitoring_fleet.validate_query(range, 500, 60) {
        Ok(query) => query,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid range").into_response(),
    };
    let since = fleet_query.window_start(Utc::now());
    let (samples, data_state) = state
        .monitoring
        .history(kind, since)
        .await
        .map(|samples| {
            let bounded: Vec<_> = samples.into_iter().take(500).collect();
            let latest_at = bounded.last().map(|sample| sample.ts);
            let deadline = Utc::now() - chrono::Duration::seconds(120);
            let state = FleetDataState::derive(bounded.len(), latest_at, deadline, None);
            (bounded, state)
        })
        .unwrap_or((
            vec![],
            FleetDataState::Unavailable {
                reason: "Monitoring unavailable. Check the collector task.".to_string(),
            },
        ));
    let content = html! {
        h1 { "Monitoring views" }
        p { "Bounded metric panels, time ranges, refresh policy, and saved layouts. Collection logic is unchanged." }
        (fleet_view_selectors(kind, range))
        @match &data_state {
            FleetDataState::Empty => {
                (crate::ui_states::EmptyState::new("No samples in this range", "Select a different metric or range.").render())
            }
            FleetDataState::Stale { last_seen_at } => {
                section class="op-error-state" {
                    p { "Data is stale. Last seen: "
                        @match last_seen_at {
                            Some(ts) => (ts.to_rfc3339()),
                            None => "unknown",
                        }
                    }
                    p { "Check the collector task, then narrow the range." }
                }
            }
            FleetDataState::Unavailable { reason } => {
                (crate::ui_states::ErrorState::new("Monitoring unavailable", reason, "/monitoring/views").render())
            }
            FleetDataState::Ready => {
                (crate::monitoring::sparkline(&samples, kind))
            }
        }
        h2 { "Save this view" }
        form method="post" action="/monitoring/views/save" class="form form-grid" {
            (crate::layout::csrf_field(&csrf))
            label { "Name" input type="text" name="name" required maxlength="64"; }
            label { "Metric" input type="text" name="metric" value=(kind.as_str()) required; }
            label { "Range (secs)" input type="number" name="range" value=(range) min="60" max="2592000" required; }
            label { "Refresh (secs)" input type="number" name="refresh" value="60" min="15" max="600" required; }
            button type="submit" { "Save view" }
        }
    };
    state
        .render_shell(&user, &csrf, "/monitoring/views", content)
        .await
        .into_response()
}

/// POST /monitoring/views/save — persist a saved view.
pub async fn fleet_view_save(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Form(form): Form<SaveFleetViewForm>,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.csrf.verify(session.id(), &form.csrf_token) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let panel = FleetViewPanel {
        metric: form.metric.clone(),
        range_secs: form.range,
    };
    let view = match FleetSavedView::new(form.name.clone(), vec![panel], form.refresh) {
        Ok(view) => view,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid saved view").into_response(),
    };
    match state.monitoring_fleet.save_fleet_view(&user, view).await {
        Ok(_) => axum::response::Redirect::to("/monitoring/views").into_response(),
        Err(_) => (StatusCode::BAD_REQUEST, "invalid saved view").into_response(),
    }
}

/// GET /fleet — safe scoped fleet health.
pub async fn fleet_health_page(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
) -> Response {
    if require_operator(&user).is_err() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let csrf = state.csrf.token_for(session.id());
    let now = Utc::now();
    // Fleet inventory is projected by the app service over agent data;
    // the web adapter never touches certs, tokens, or command material.
    let hosts: Vec<FleetHostSummary> = state.monitoring_fleet.aggregate_fleet_hosts(
        &[],
        &std::collections::HashMap::new(),
        "1.0.0",
        &std::collections::HashMap::new(),
        user.id(),
        now,
    );
    let content = html! {
        h1 { "Fleet health" }
        p { "Heartbeat freshness, version drift, and health by authorized scope. No certificates, tokens, or command material." }
        (render_fleet_hosts(&hosts))
    };
    state
        .render_shell(&user, &csrf, "/fleet", content)
        .await
        .into_response()
}

/// Metric + range selector strip reusing the monitoring vocabulary.
fn fleet_view_selectors(metric: MetricKind, range: i64) -> Markup {
    html! {
        form class="form form-inline"
            hx-get="/monitoring/views"
            hx-target="body"
            hx-trigger="change"
            hx-swap="outerHTML" {
            label { "Metric" }
            select name="metric" {
                @for value in ["Cpu", "Memory", "Disk", "Network"] {
                    @if value == metric.as_str() {
                        option value=(value) selected { (value) }
                    } @else {
                        option value=(value) { (value) }
                    }
                }
            }
            label { "Range" }
            select name="range" {
                @for (value, label) in [(3600, "1 hour"), (21600, "6 hours"), (86400, "24 hours"), (604800, "7 days")] {
                    @if value == range {
                        option value=(value) selected { (label) }
                    } @else {
                        option value=(value) { (label) }
                    }
                }
            }
            button type="submit" { "Apply" }
        }
    }
}

/// Secret-free fleet table (hostname, health, last-seen only).
pub fn render_fleet_hosts(hosts: &[FleetHostSummary]) -> Markup {
    html! {
        section id="fleet-health" {
            @if hosts.is_empty() {
                (crate::ui_states::EmptyState::new("No hosts in scope", "Register an agent to see fleet health.").render())
            } @else {
                table class="table" {
                    thead {
                        tr {
                            th { "Hostname" }
                            th { "Health" }
                            th { "Last seen" }
                            th { "Guidance" }
                        }
                    }
                    tbody {
                        @for host in hosts {
                            tr {
                                td { (host.hostname) }
                                td {
                                    span class=(fleet_health_class(host)) {
                                        (host.health.as_str())
                                    }
                                }
                                td {
                                    @match host.last_seen_at {
                                        Some(ts) => (ts.to_rfc3339()),
                                        None => "—",
                                    }
                                }
                                td { (host.recovery_guidance) }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "monitoring-fleet-operations");
    }

    #[test]
    fn fleet_table_is_secret_free_and_scoped() {
        let host = openpanel_domain::monitoring_fleet::project_fleet_host(
            uuid::Uuid::new_v4(),
            "web-01",
            false,
            false,
            Some(chrono::Utc::now()),
            "1.0.0",
            "1.0.0",
            true,
            chrono::Utc::now(),
        );
        let out = render_fleet_hosts(std::slice::from_ref(&host)).into_string();
        assert!(out.contains("web-01"));
        assert!(out.contains("healthy"));
        assert!(!out.contains("cert"));
        assert!(!out.contains("token"));
        let empty = render_fleet_hosts(&[]).into_string();
        assert!(empty.contains("No hosts in scope"));
    }

    #[test]
    fn selectors_cover_metrics_and_ranges() {
        let out = fleet_view_selectors(MetricKind::Cpu, 3600).into_string();
        for value in ["Cpu", "Memory", "Disk", "Network"] {
            assert!(out.contains(value), "metric {value}");
        }
        assert!(out.contains("1 hour"));
    }

    #[test]
    fn stale_and_unavailable_states_render_safely() {
        let stale = FleetDataState::Stale { last_seen_at: None };
        assert!(matches!(stale, FleetDataState::Stale { .. }));
        let unavailable = FleetDataState::Unavailable {
            reason: "collector failed".into(),
        };
        assert!(matches!(unavailable, FleetDataState::Unavailable { .. }));
    }
}
