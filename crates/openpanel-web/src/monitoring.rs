//! Monitoring pages: history sparkline, alert feed, and the landing page
//! that ties them together — all rendered server-side in the shell.
//!
//! Charts are SVG polylines rendered directly from the `MetricSample`
//! values returned by `MonitoringService::history`. No JavaScript charting
//! library; the page stays attribute-driven.
//!
//! HTMX swap targets:
//! * `#history-chart` — fragment from `GET /monitoring/history?...`
//! * `#alert-feed`    — fragment from `GET /monitoring/alerts`, auto-refreshes
//!   every 60 s.

use std::str::FromStr;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::{Duration, Utc};
use maud::{Markup, html};
use openpanel_core::{AuditAction, AuditEvent};
use openpanel_domain::monitoring::{MetricKind, MetricSample, Unit};
use serde::Deserialize;

use crate::router::{WebState, WebUser};

/// Width of the rendered SVG sparkline in user units.
const CHART_WIDTH: f64 = 600.0;
/// Height of the rendered SVG sparkline in user units.
const CHART_HEIGHT: f64 = 160.0;
/// Y-axis maximum for percent-based metrics.
const PERCENT_MAX: f64 = 100.0;

/// Available metric options shown in the selector, in display order.
pub const METRIC_OPTIONS: &[(&str, &str)] = &[
    ("Cpu", "Cpu"),
    ("Memory", "Memory"),
    ("Disk", "Disk"),
    ("Network", "Network"),
];

/// Available range options shown in the selector, expressed in seconds.
pub const RANGE_OPTIONS: &[(&str, &str, i64)] = &[
    ("1h", "1 hour", 3600),
    ("6h", "6 hours", 6 * 3600),
    ("24h", "24 hours", 24 * 3600),
    ("7d", "7 days", 7 * 24 * 3600),
];

/// Query parameters for `GET /monitoring/history`.
#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    /// Metric kind identifier (e.g. `Cpu`).
    pub metric: Option<String>,
    /// Look-back window in seconds (default 3600).
    pub range: Option<i64>,
}

/// One alert row for rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertView {
    /// Metric that fired (e.g. `Disk`).
    pub metric: String,
    /// Measured value at firing time.
    pub value: f64,
    /// Configured threshold.
    pub threshold: f64,
}

/// GET /monitoring — the monitoring landing page.
pub async fn landing(State(state): State<WebState>, WebUser(user, session): WebUser) -> Response {
    let csrf = state.csrf.token_for(session.id());
    let metric = MetricKind::Cpu;
    let range = 3600_i64;
    let content = html! {
        h1 { "Monitoring" }
        (selectors(metric, range))
        section id="history-chart"
            hx-get=(format!("/monitoring/history?metric={}&range={range}", metric.as_str()))
            hx-trigger="load"
            hx-swap="outerHTML" {
            (crate::ui_states::LoadingState::new("Loading history…").render())
        }
        h2 { "Recent alerts" }
        section id="alert-feed"
            hx-get="/monitoring/alerts"
            hx-trigger="load, every 60s"
            hx-swap="outerHTML" {
            (crate::ui_states::LoadingState::new("Loading alerts…").render())
        }
    };
    state
        .render_shell(&user, &csrf, "/monitoring", content)
        .await
        .into_response()
}

/// GET /monitoring/history?metric=&range= — the SVG sparkline fragment.
pub async fn history(
    State(state): State<WebState>,
    WebUser(_user, _session): WebUser,
    Query(q): Query<HistoryQuery>,
) -> Response {
    let metric_str = q.metric.unwrap_or_else(|| "Cpu".to_string());
    let kind = match MetricKind::from_str(&metric_str) {
        Ok(k) => k,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid metric").into_response(),
    };
    let range = q.range.unwrap_or(3600).max(1);
    let since = Utc::now() - Duration::seconds(range);
    let samples = state
        .monitoring
        .history(kind, since)
        .await
        .unwrap_or_default();
    history_fragment(&samples, kind).into_response()
}

/// GET /monitoring/alerts — the alert feed fragment.
pub async fn alerts(State(state): State<WebState>, WebUser(_user, _session): WebUser) -> Response {
    let events = state.monitoring.audit_events(100).await.unwrap_or_default();
    let alerts: Vec<AlertView> = events
        .iter()
        .filter(|e| e.action == AuditAction::AlertFired)
        .map(alert_from_event)
        .collect();
    alert_fragment(&alerts).into_response()
}

/// Render the metric + range selector strip. Changes `hx-get` the chart
/// partial — no page navigation.
fn selectors(metric: MetricKind, range: i64) -> Markup {
    html! {
        form class="form form-inline"
            hx-get="/monitoring/history"
            hx-target="#history-chart"
            hx-trigger="change"
            hx-swap="outerHTML" {
            label { "Metric" }
            select name="metric" {
                @for (value, label) in METRIC_OPTIONS {
                    @if *value == metric.as_str() {
                        option value=(value) selected { (label) }
                    } @else {
                        option value=(value) { (label) }
                    }
                }
            }
            label { "Range" }
            select name="range" {
                @for (_, label, seconds) in RANGE_OPTIONS {
                    @if *seconds == range {
                        option value=(seconds) selected { (label) }
                    } @else {
                        option value=(seconds) { (label) }
                    }
                }
            }
            button type="submit" { "Apply" }
        }
    }
}

/// Render the SVG sparkline for a set of samples, or the placeholder
/// when no samples exist.
pub fn history_fragment(samples: &[MetricSample], kind: MetricKind) -> Markup {
    html! {
        section id="history-chart" {
            h2 { (kind.as_str()) " — last "
                (humanize_range(samples))
            }
            @if samples.is_empty() {
                (crate::ui_states::EmptyState::new("No samples in this range", "Select a different metric or range.").render())
            } @else {
                (sparkline(samples, kind))
            }
        }
    }
}

/// Render the SVG `<svg>` + `<polyline>` for a series of samples.
///
/// The y-axis is normalized to the sample max (or 100 for percent
/// metrics); the x-axis spreads points evenly across the chart width.
/// Numeric values are sanitized: every coordinate is finite and clamped
/// to the chart's user-unit box before emission.
pub fn sparkline(samples: &[MetricSample], kind: MetricKind) -> Markup {
    if samples.is_empty() {
        return crate::ui_states::EmptyState::new(
            "No samples in this range",
            "Select a different metric or range.",
        )
        .render();
    }
    let unit = samples.first().map(|s| s.unit).unwrap_or(Unit::Percent);
    let ymax = match unit {
        Unit::Percent => PERCENT_MAX,
        _ => samples
            .iter()
            .map(|s| s.value)
            .fold(f64::NEG_INFINITY, f64::max)
            .max(1.0),
    };
    let n = samples.len();
    let step = if n > 1 {
        CHART_WIDTH / (n as f64 - 1.0)
    } else {
        0.0
    };
    let mut points = String::new();
    for (i, s) in samples.iter().enumerate() {
        let x = step * i as f64;
        let y_norm = (s.value / ymax).clamp(0.0, 1.0);
        let y = CHART_HEIGHT - y_norm * CHART_HEIGHT;
        let x_safe = if x.is_finite() { x } else { 0.0 };
        let y_safe = if y.is_finite() { y } else { CHART_HEIGHT };
        let x_safe = x_safe.clamp(0.0, CHART_WIDTH);
        let y_safe = y_safe.clamp(0.0, CHART_HEIGHT);
        if i > 0 {
            points.push(' ');
        }
        points.push_str(&format!("{x_safe:.1},{y_safe:.1}"));
    }
    html! {
        svg class="sparkline"
            viewBox=(format!("0 0 {CHART_WIDTH} {CHART_HEIGHT}"))
            width=(format!("{CHART_WIDTH}"))
            height=(format!("{CHART_HEIGHT}"))
            role="img"
            aria-label=(format!("{} sparkline", kind.as_str())) {
            polyline fill="none" stroke="var(--accent)" stroke-width="2" points=(points);
        }
    }
}

/// Render the alert feed, or the empty state when there are no alerts.
pub fn alert_fragment(alerts: &[AlertView]) -> Markup {
    html! {
        section id="alert-feed" {
            @if alerts.is_empty() {
                (crate::ui_states::EmptyState::new("No alerts", "Alert events appear here when thresholds are crossed.").render())
            } @else {
                table class="table alerts" {
                    thead {
                        tr {
                            th { "Metric" }
                            th { "Value" }
                            th { "Threshold" }
                        }
                    }
                    tbody {
                        @for a in alerts {
                            tr {
                                td { (a.metric) }
                                td { (format!("{:.1}", a.value)) }
                                td { (format!("{:.1}", a.threshold)) }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Convert an `AuditEvent` for `AlertFired` into a display row.
fn alert_from_event(event: &AuditEvent) -> AlertView {
    let metric = event.target.clone().unwrap_or_default();
    let value = event
        .metadata
        .get("value")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let threshold = event
        .metadata
        .get("threshold")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    AlertView {
        metric,
        value,
        threshold,
    }
}

/// Map a sample range to a human label.
fn humanize_range(samples: &[MetricSample]) -> String {
    if samples.is_empty() {
        return "range".to_string();
    }
    let first = samples.first().map(|s| s.ts).unwrap_or_else(Utc::now);
    let last = samples.last().map(|s| s.ts).unwrap_or(first);
    let span = last - first;
    if span < Duration::hours(1) {
        format!("{} minutes", span.num_minutes().max(1))
    } else if span < Duration::days(1) {
        format!("{} hours", span.num_hours())
    } else {
        format!("{} days", span.num_days().max(1))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use openpanel_core::{AuditAction, AuditEvent, AuditOutcome};
    use openpanel_domain::monitoring::{MetricKind, MetricSample, Unit};

    use super::*;

    fn sample(value: f64, dt: chrono::DateTime<Utc>) -> MetricSample {
        MetricSample::new(MetricKind::Cpu, Unit::Percent, value, dt).expect("sample")
    }

    #[test]
    fn sparkline_renders_polyline_points_from_samples() {
        let now = Utc::now();
        let samples = vec![
            sample(10.0, now),
            sample(50.0, now + Duration::seconds(30)),
            sample(80.0, now + Duration::seconds(60)),
        ];
        let out = sparkline(&samples, MetricKind::Cpu).into_string();
        assert!(out.contains("<polyline"), "polyline: {out}");
        assert!(out.contains("points="), "points attribute: {out}");
        // 3 points, separated by spaces, each in `x,y` form.
        let points_start = out.find("points=\"").expect("points attr") + 8;
        let points_end = out[points_start..].find('"').expect("closing quote");
        let points_str = &out[points_start..points_start + points_end];
        let points: Vec<&str> = points_str.split_whitespace().collect();
        assert_eq!(points.len(), 3, "three points: {out}");
        for p in &points {
            let mut coords = p.split(',');
            let x: f64 = coords.next().expect("x").parse().expect("finite x");
            let y: f64 = coords.next().expect("y").parse().expect("finite y");
            assert!(
                x.is_finite() && (0.0..=CHART_WIDTH).contains(&x),
                "x sane: {p}"
            );
            assert!(
                y.is_finite() && (0.0..=CHART_HEIGHT).contains(&y),
                "y sane: {p}"
            );
        }
    }

    #[test]
    fn sparkline_empty_returns_placeholder() {
        let out = sparkline(&[], MetricKind::Cpu).into_string();
        assert!(out.contains("No samples"), "placeholder: {out}");
    }

    #[test]
    fn history_fragment_renders_placeholder_when_no_samples() {
        let out = history_fragment(&[], MetricKind::Memory).into_string();
        assert!(out.contains("No samples"), "placeholder: {out}");
        assert!(out.contains("id=\"history-chart\""), "target id: {out}");
    }

    #[test]
    fn history_fragment_renders_chart_with_samples() {
        let now = Utc::now();
        let samples = vec![sample(20.0, now), sample(40.0, now + Duration::seconds(60))];
        let out = history_fragment(&samples, MetricKind::Cpu).into_string();
        assert!(out.contains("id=\"history-chart\""), "target id: {out}");
        assert!(out.contains("<polyline"), "polyline: {out}");
    }

    #[test]
    fn selectors_render_all_metric_and_range_options() {
        let out = selectors(MetricKind::Cpu, 3600).into_string();
        for value in ["Cpu", "Memory", "Disk", "Network"] {
            assert!(out.contains(value), "metric {value}: {out}");
        }
        for label in ["1 hour", "6 hours", "24 hours", "7 days"] {
            assert!(out.contains(label), "range {label}: {out}");
        }
        assert!(out.contains("name=\"metric\""), "metric name: {out}");
        assert!(out.contains("name=\"range\""), "range name: {out}");
    }

    #[test]
    fn alert_fragment_empty_renders_no_alerts() {
        let out = alert_fragment(&[]).into_string();
        assert!(out.contains("No alerts"), "empty state: {out}");
        assert!(out.contains("id=\"alert-feed\""), "target id: {out}");
    }

    #[test]
    fn alert_fragment_renders_metric_value_threshold() {
        let alerts = vec![AlertView {
            metric: "Disk".into(),
            value: 91.5,
            threshold: 90.0,
        }];
        let out = alert_fragment(&alerts).into_string();
        assert!(out.contains("Disk"), "metric: {out}");
        assert!(out.contains("91.5"), "value: {out}");
        assert!(out.contains("90.0"), "threshold: {out}");
    }

    #[test]
    fn alert_from_event_extracts_metadata() {
        let event = AuditEvent::new("monitoring", AuditAction::AlertFired, AuditOutcome::Success)
            .target("Memory")
            .metadata(serde_json::json!({ "value": 88.0, "threshold": 80.0 }));
        let view = alert_from_event(&event);
        assert_eq!(view.metric, "Memory");
        assert_eq!(view.value, 88.0);
        assert_eq!(view.threshold, 80.0);
    }

    #[test]
    fn parse_metric_kind_recognises_known_values() {
        for (raw, expected) in [
            ("Cpu", MetricKind::Cpu),
            ("cpu", MetricKind::Cpu),
            ("Memory", MetricKind::Memory),
            ("memory", MetricKind::Memory),
            ("Disk", MetricKind::Disk),
            ("Network", MetricKind::Network),
        ] {
            assert_eq!(MetricKind::from_str(raw).expect("known"), expected);
        }
        assert!(MetricKind::from_str("Bogus").is_err());
    }
}
