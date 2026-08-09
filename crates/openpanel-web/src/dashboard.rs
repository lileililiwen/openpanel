//! Dashboard page: live host gauges, resource quick-count cards, and the
//! recent-alerts feed, all rendered server-side inside the shell.
//!
//! Data flows through services, never HTTP: gauges come from
//! `MonitoringService::snapshot_now`, counts from the resource services'
//! list methods, and alerts from the monitoring alert history (the same
//! `AlertFired` audit events the API's `/monitoring/alerts` endpoint returns).

use axum::extract::State;
use chrono::Utc;
use maud::{Markup, html};
use openpanel_core::{AuditAction, AuditEvent};
use openpanel_domain::{
    User,
    files::path::Path as FilePath,
    monitoring::{DiskReading, SystemSnapshot},
};

use crate::router::{WebState, WebUser};

/// Counts backing the five quick-count cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceCounts {
    /// Number of sites visible to the caller.
    pub sites: usize,
    /// Number of databases visible to the caller.
    pub databases: usize,
    /// Number of files across all visible site document roots.
    pub files: usize,
    /// Number of SSL certificates.
    pub ssl: usize,
    /// Number of users.
    pub users: usize,
}

/// One recent alert event, derived from the audit log.
#[derive(Debug, Clone, PartialEq)]
pub struct AlertView {
    /// Metric that crossed its threshold (e.g. `Disk`).
    pub metric: String,
    /// Measured value at firing time.
    pub value: f64,
    /// Configured threshold.
    pub threshold: f64,
}

/// GET / and GET /dashboard — the dashboard page inside the shell.
pub async fn home(State(state): State<WebState>, WebUser(user, session): WebUser) -> Markup {
    let csrf = state.csrf.token_for(session.id());
    let snapshot = match state.monitoring.snapshot_now().await {
        Ok(s) => s,
        Err(_) => fallback_snapshot(),
    };
    let counts = collect_counts(&state, &user).await;
    let alerts = collect_alerts(&state).await;
    let content = html! {
        h1 { "Dashboard" }
        (gauges_section(&snapshot))
        (cards_section(&counts))
        (alerts_section(&alerts))
    };
    state.render_shell(&user, &csrf, "/", content).await
}

/// GET /dashboard/gauges — the auto-refreshed `#host-gauges` partial.
pub async fn gauges_partial(State(state): State<WebState>, _user: WebUser) -> Markup {
    let snapshot = match state.monitoring.snapshot_now().await {
        Ok(s) => s,
        Err(_) => fallback_snapshot(),
    };
    gauges_section(&snapshot)
}

/// Render the `#host-gauges` region: CPU, memory, disk (max mount percent),
/// and the 1-minute load average as CSS-width bars. Carries the HTMX
/// auto-refresh attributes so the partial re-fetches itself every 30 s.
pub fn gauges_section(snapshot: &SystemSnapshot) -> Markup {
    let cpu = clamp_percent(snapshot.cpu);
    let memory = clamp_percent(snapshot.memory);
    let disk = snapshot.disk.iter().map(|d| d.percent).fold(0.0, f64::max);
    let disk = clamp_percent(disk);
    let load = snapshot.load.max(0.0);
    html! {
        section id="host-gauges" class="gauges" hx-get="/dashboard/gauges" hx-trigger="every 30s" hx-swap="outerHTML" {
            div class="gauge" {
                span class="gauge-label" { "CPU" }
                div class="gauge-bar" {
                    div class="gauge-fill" style=(format!("width: {cpu:.1}%"));
                }
                span class="gauge-value" { (format!("{cpu:.1}%")) }
            }
            div class="gauge" {
                span class="gauge-label" { "Memory" }
                div class="gauge-bar" {
                    div class="gauge-fill" style=(format!("width: {memory:.1}%"));
                }
                span class="gauge-value" { (format!("{memory:.1}%")) }
            }
            div class="gauge" {
                span class="gauge-label" { "Disk" }
                div class="gauge-bar" {
                    div class="gauge-fill" style=(format!("width: {disk:.1}%"));
                }
                span class="gauge-value" { (format!("{disk:.1}%")) }
            }
            div class="gauge" {
                span class="gauge-label" { "Load" }
                span class="gauge-value" { (format!("{load:.2}")) }
            }
        }
    }
}

/// Render the five quick-count cards, each linking to its resource page.
pub fn cards_section(counts: &ResourceCounts) -> Markup {
    let cards = [
        ("/sites", "Sites", counts.sites),
        ("/databases", "Databases", counts.databases),
        ("/files", "Files", counts.files),
        ("/ssl", "SSL", counts.ssl),
        ("/users", "Users", counts.users),
    ];
    html! {
        section class="cards" {
            h2 { "Resources" }
            div class="card-grid" {
                @for (href, label, count) in cards {
                    a class="card" href=(href) {
                        span class="card-count" { (count) }
                        span class="card-label" { (label) }
                    }
                }
            }
        }
    }
}

/// Render the recent-alerts list, or the "No alerts" empty state.
pub fn alerts_section(alerts: &[AlertView]) -> Markup {
    html! {
        section class="alerts" {
            h2 { "Recent alerts" }
            @if alerts.is_empty() {
                p class="empty" { "No alerts" }
            } @else {
                ul {
                    @for alert in alerts {
                        li {
                            span class="alert-metric" { (alert.metric) }
                            span class="alert-value" {
                                (format!("{:.1} above {:.1}", alert.value, alert.threshold))
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Count sites, databases, files (top-level entries across site roots),
/// SSL certs, and users for the caller.
async fn collect_counts(state: &WebState, caller: &User) -> ResourceCounts {
    let sites = state
        .sites
        .list_sites(caller)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let mut files = 0;
    if let Ok(site_list) = state.sites.list_sites(caller).await {
        for site in &site_list {
            if let Ok(entries) = state
                .files
                .list_dir(caller, site.id(), &FilePath::root())
                .await
            {
                files += entries.len();
            }
        }
    }
    let databases = state
        .databases
        .list_databases(caller)
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    let ssl = state.ssl.list().await.map(|v| v.len()).unwrap_or(0);
    let users = state
        .identity
        .list_users()
        .await
        .map(|v| v.len())
        .unwrap_or(0);
    ResourceCounts {
        sites,
        databases,
        files,
        ssl,
        users,
    }
}

/// The most recent `AlertFired` audit events (newest first), mapped to
/// lightweight view structs.
async fn collect_alerts(state: &WebState) -> Vec<AlertView> {
    let events = state.monitoring.audit_events(20).await.unwrap_or_default();
    events
        .iter()
        .filter(|e| e.action == AuditAction::AlertFired)
        .map(alert_from_event)
        .collect()
}

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

/// A valid fallback snapshot for when collection fails (so the page still
/// renders rather than 500ing).
fn fallback_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        timestamp: Utc::now(),
        load: 0.0,
        cpu: 0.0,
        memory: 0.0,
        disk: vec![DiskReading {
            mount: "/".into(),
            percent: 0.0,
        }],
        network: vec![],
    }
}

fn clamp_percent(v: f64) -> f64 {
    if !v.is_finite() {
        0.0
    } else {
        v.clamp(0.0, 100.0)
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn snapshot() -> SystemSnapshot {
        SystemSnapshot::new(
            Utc::now(),
            1.25,
            42.3,
            57.8,
            vec![DiskReading {
                mount: "/".into(),
                percent: 81.4,
            }],
            vec![],
        )
        .expect("valid snapshot")
    }

    #[test]
    fn gauges_render_all_values() {
        let out = gauges_section(&snapshot()).into_string();
        assert!(out.contains("42.3%"), "cpu gauge: {out}");
        assert!(out.contains("57.8%"), "memory gauge: {out}");
        assert!(out.contains("81.4%"), "disk gauge: {out}");
        assert!(out.contains("1.25"), "load figure: {out}");
        assert!(out.contains("id=\"host-gauges\""), "region id present");
        assert!(
            out.contains("hx-trigger=\"every 30s\""),
            "auto-refresh trigger"
        );
    }

    #[test]
    fn cards_render_counts_and_links() {
        let counts = ResourceCounts {
            sites: 3,
            databases: 2,
            files: 14,
            ssl: 1,
            users: 5,
        };
        let out = cards_section(&counts).into_string();
        for (href, label, count) in [
            ("/sites", "Sites", 3),
            ("/databases", "Databases", 2),
            ("/files", "Files", 14),
            ("/ssl", "SSL", 1),
            ("/users", "Users", 5),
        ] {
            assert!(out.contains(&format!("href=\"{href}\"")), "link {href}");
            assert!(out.contains(label), "label {label}");
            assert!(
                out.contains(&format!("{count}")),
                "count {count} for {label}"
            );
        }
    }

    #[test]
    fn alerts_render_metric_value_threshold() {
        let alerts = vec![AlertView {
            metric: "Disk".into(),
            value: 91.5,
            threshold: 90.0,
        }];
        let out = alerts_section(&alerts).into_string();
        assert!(out.contains("Disk"), "metric: {out}");
        assert!(out.contains("91.5 above 90.0"), "value/threshold: {out}");
    }

    #[test]
    fn alerts_empty_state() {
        let out = alerts_section(&[]).into_string();
        assert!(out.contains("No alerts"), "empty state: {out}");
    }

    #[test]
    fn fallback_snapshot_constructs() {
        let snap = fallback_snapshot();
        assert_eq!(snap.cpu, 0.0);
        assert!(!snap.disk.is_empty());
    }

    #[test]
    fn alert_from_event_extracts_metadata() {
        let event = AuditEvent::new(
            "monitoring",
            AuditAction::AlertFired,
            openpanel_core::AuditOutcome::Success,
        )
        .target("Cpu")
        .metadata(serde_json::json!({ "value": 88.0, "threshold": 80.0 }));
        let view = alert_from_event(&event);
        assert_eq!(view.metric, "Cpu");
        assert_eq!(view.value, 88.0);
        assert_eq!(view.threshold, 80.0);
    }
}
