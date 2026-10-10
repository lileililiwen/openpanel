//! Role-aware operations dashboard, rendered server-side inside the shell.
//!
//! Data flows through services, never HTTP: resource measurements come from
//! `MonitoringService`, counts from the resource services' list methods,
//! alerts from the monitoring alert history, and the attention queue from the
//! security, system-service, and backup services.
//!
//! Every auto-refreshed region carries an `aria-live` / busy state and renders
//! an explicit error (never a silent zero) when collection fails.

use std::collections::HashSet;

use axum::extract::{Query, State};
use chrono::{Duration, Utc};
use maud::{Markup, html};
use openpanel_app::software_center::CatalogQuery;
use openpanel_core::{AuditAction, AuditEvent};
use openpanel_domain::{
    User,
    backups::BackupRunState,
    files::path::Path as FilePath,
    identity::Role,
    monitoring::{MetricKind, MetricSample, SystemSnapshot},
};
use serde::Deserialize;

use crate::monitoring::sparkline;
use crate::router::{WebState, WebUser};
use crate::ui_states::{EmptyState, ErrorState};

mod widgets;
pub use widgets::{
    QuotaSummary, SecurityRiskCard, SoftwareShortcut, TrafficSummary, count_ftp_accounts,
    dashboard_links, panel_shortcuts_for, panel_shortcuts_section, quota_section,
    quota_summary_from, security_risk_card, security_risk_section, software_section,
    software_shortcuts, traffic_summary,
};

/// How long before a collected snapshot is considered stale.
const STALE_THRESHOLD: Duration = Duration::minutes(5);
/// Disk/memory utilization at or above this is "degraded".
const DEGRADED_THRESHOLD: f64 = 85.0;

/// Explicit health of a single resource widget. Never rely on color alone:
/// every variant carries a text label surfaced in the markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceStatus {
    /// Within healthy bounds.
    Healthy,
    /// Above the degraded threshold but still reporting.
    Degraded,
    /// Source reported no reading.
    Unknown,
    /// Collection failed; an error state is shown.
    Error,
}

impl ResourceStatus {
    /// Human-readable label, used as visible text (not only color).
    pub fn label(self) -> &'static str {
        match self {
            ResourceStatus::Healthy => "Healthy",
            ResourceStatus::Degraded => "Degraded",
            ResourceStatus::Unknown => "Unknown",
            ResourceStatus::Error => "Unavailable",
        }
    }

    /// Stable CSS hook (reuses the panel status vocabulary, no new palette).
    pub fn class(self) -> &'static str {
        match self {
            ResourceStatus::Healthy => "op-status-healthy",
            ResourceStatus::Degraded => "op-status-degraded",
            ResourceStatus::Unknown => "op-status-unknown",
            ResourceStatus::Error => "op-status-error",
        }
    }
}

/// Severity of an attention item in the operator queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// Action required now (failed job, down service).
    Critical,
    /// Worth a look (active block, warning).
    Warning,
    /// Informational.
    Info,
}

impl Severity {
    fn class(self) -> &'static str {
        match self {
            Severity::Critical => "op-attn-critical",
            Severity::Warning => "op-attn-warning",
            Severity::Info => "op-attn-info",
        }
    }
}

/// One percentage gauge widget (CPU / Memory / Disk-max).
#[derive(Debug, Clone, PartialEq)]
pub struct GaugeWidget {
    /// Resource label.
    pub label: &'static str,
    /// Utilization percentage in `[0, 100]`.
    pub percent: f64,
    /// Derived status.
    pub status: ResourceStatus,
}

/// One mounted filesystem capacity row.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskMountWidget {
    /// Mount point.
    pub mount: String,
    /// Utilization percentage in `[0, 100]`.
    pub percent: f64,
    /// Derived status.
    pub status: ResourceStatus,
}

/// One network interface throughput row.
#[derive(Debug, Clone, PartialEq)]
pub struct NetworkWidget {
    /// Interface name.
    pub interface: String,
    /// Inbound bytes/sec.
    pub rx: u64,
    /// Outbound bytes/sec.
    pub tx: u64,
}

/// One item in the operator attention queue.
#[derive(Debug, Clone, PartialEq)]
pub struct AttentionItem {
    /// Severity.
    pub severity: Severity,
    /// Short label.
    pub label: String,
    /// Supporting detail.
    pub detail: String,
    /// Optional destination for remediation.
    pub href: Option<String>,
}

/// One contextual quick action linking to an existing workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickAction {
    /// Action label.
    pub label: &'static str,
    /// Destination route (already exists).
    pub href: &'static str,
}

/// Counts backing the resource quick-count cards.
#[derive(Debug, Clone, PartialEq)]
pub struct ResourceCounts {
    /// Number of sites visible to the caller.
    pub sites: usize,
    /// Number of FTP accounts across caller-visible sites.
    pub ftp: usize,
    /// Set when FTP collection failed; the card shows it, never zero.
    pub ftp_error: Option<String>,
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

/// Aggregated, render-ready operations dashboard model. Built from existing
/// services; each region is collected independently so a failure isolates to
/// its own error widget rather than poisoning the whole page.
#[derive(Debug, Clone, PartialEq)]
pub struct DashboardModel {
    /// Server/panel identity.
    pub server_name: String,
    /// Operating system.
    pub os: String,
    /// CPU architecture.
    pub arch: String,
    /// Panel version string.
    pub panel_version: &'static str,
    /// When the model's primary snapshot was collected.
    pub last_updated: chrono::DateTime<Utc>,
    /// CPU / Memory / Disk gauges.
    pub gauges: Vec<GaugeWidget>,
    /// 1-minute load average.
    pub load: f64,
    /// Per-mount disk capacity rows.
    pub disk_mounts: Vec<DiskMountWidget>,
    /// Per-interface network rows.
    pub network: Vec<NetworkWidget>,
    /// Resource quick-counts.
    pub counts: ResourceCounts,
    /// Recent alerts.
    pub alerts: Vec<AlertView>,
    /// Operator attention queue.
    pub attention: Vec<AttentionItem>,
    /// Contextual quick actions.
    pub quick_actions: Vec<QuickAction>,
    /// Set when the live monitoring snapshot could not be collected.
    pub monitoring_error: Option<String>,
    /// Recent CPU samples for the trend panel.
    pub cpu_trend: Vec<MetricSample>,
    /// Whether the caller is an owner (gates owner-only cards).
    pub is_owner: bool,
    /// Security-risk card state.
    pub security_risk: SecurityRiskCard,
    /// Pinned software shortcuts (Display-on-dashboard subset).
    pub software: Vec<SoftwareShortcut>,
    /// Set when the software catalog could not be searched.
    pub software_error: Option<String>,
    /// Traffic totals over the network rows.
    pub traffic: TrafficSummary,
    /// Panel Update/Fix/Restart deep-links (owner-only).
    pub panel_shortcuts: Vec<QuickAction>,
    /// Caller-scoped quota summary.
    pub quota: QuotaSummary,
}

/// Dashboard query: `?pin=<entry-id>` repeats select Display-on-dashboard
/// entries. The toggle filters the rendered shortcuts; it never mutates
/// the catalog.
#[derive(Debug, Deserialize, Default)]
pub struct DashboardQuery {
    /// Display-on-dashboard entry ids (comma-separated `?pin=a,b`).
    #[serde(default)]
    pub pin: Option<String>,
}

/// GET / and GET /dashboard — the dashboard page inside the shell.
pub async fn home(
    State(state): State<WebState>,
    WebUser(user, session): WebUser,
    Query(query): Query<DashboardQuery>,
) -> Markup {
    let csrf = state.csrf.token_for(session.id());
    let pinned: HashSet<String> = query
        .pin
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let model = build_dashboard(&state, &user, &pinned).await;
    state
        .render_shell(&user, &csrf, "/", dashboard_content(&model))
        .await
}

/// GET /dashboard/gauges — the auto-refreshed `#host-gauges` partial.
pub async fn gauges_partial(State(state): State<WebState>, _user: WebUser) -> Markup {
    match state.monitoring.snapshot_now().await {
        Ok(snapshot) => {
            let (gauges, load) = gauges_from_snapshot(&snapshot);
            gauges_region(&gauges, load, None)
        }
        Err(e) => gauges_region(&[], 0.0, Some(format!("Monitoring unavailable: {e}"))),
    }
}

/// Render the full dashboard body from the model.
pub fn dashboard_content(model: &DashboardModel) -> Markup {
    html! {
        h1 { "Dashboard" }
        (header_section(model))
        (attention_section(&model.attention))
        (gauges_region(&model.gauges, model.load, model.monitoring_error.clone()))
        (resource_grid_section(model))
        (trend_section(&model.cpu_trend))
        (cards_section(&model.counts))
        (security_risk_section(&model.security_risk, model.is_owner))
        @if model.is_owner {
            (software_section(&model.software, model.software_error.clone()))
        }
        (panel_shortcuts_section(&model.panel_shortcuts))
        (quota_section(&model.quota))
        (alerts_section(&model.alerts))
        (quick_actions_section(&model.quick_actions))
    }
}

/// Page header: server identity, environment, last refresh, and a refresh
/// action. Shows an explicit "stale" marker when the snapshot is old.
pub fn header_section(model: &DashboardModel) -> Markup {
    let stale = is_stale(model.last_updated);
    let updated = model.last_updated.to_rfc3339();
    html! {
        section id="dashboard-header" class="dashboard-header" {
            div class="server-identity" {
                h2 { (model.server_name) }
                dl class="server-meta" {
                    dt { "OS" }
                    dd { (model.os) " (" (model.arch) ")" }
                    dt { "Panel" }
                    dd { (model.panel_version) }
                }
            }
            div class="dashboard-refresh" {
                p class="last-updated" {
                    "Last updated: "
                    time datetime=(updated)
                        class=(if stale { "op-status-stale" } else { "op-status-fresh" }) {
                        (updated)
                    }
                    @if stale {
                        span class="stale-flag" { " (stale)" }
                    }
                }
                a class="btn" href="/" hx-get="/" hx-target="body" hx-swap="outerHTML" {
                    "Refresh"
                }
            }
        }
    }
}

/// Render the operator attention queue, or an all-clear empty state.
pub fn attention_section(items: &[AttentionItem]) -> Markup {
    html! {
        section id="attention-queue" class="attention-queue" aria-live="polite" {
            h2 { "Attention" }
            @if items.is_empty() {
                (EmptyState::new(
                    "Nothing needs attention",
                    "Security, services, and backups are all clear for your scope."
                ).render())
            } @else {
                ul {
                    @for item in items {
                        li class=(item.severity.class()) {
                            span class="attn-label" { (item.label) }
                            span class="attn-detail" { (item.detail) }
                            @if let Some(href) = &item.href {
                                a class="attn-action" href=(href) { "Review" }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Render the `#host-gauges` region: CPU, memory, disk gauges plus the load
/// figure. On collection failure, renders an error state with retry guidance
/// instead of silently emitting zeros.
pub fn gauges_region(gauges: &[GaugeWidget], load: f64, error: Option<String>) -> Markup {
    html! {
        section id="host-gauges" class="gauges" hx-get="/dashboard/gauges" hx-trigger="every 30s" hx-swap="outerHTML" aria-live="polite" {
            h2 { "Host health" }
            @if let Some(msg) = &error {
                (ErrorState::new(
                    "Live metrics unavailable",
                    msg,
                    "/dashboard/gauges"
                ).render())
            } @else {
                div class="gauge-grid" {
                    @for g in gauges {
                        div class="gauge" {
                            span class="gauge-label" { (g.label) }
                            div class="gauge-bar" {
                                div class=(format!("gauge-fill {}", g.status.class()))
                                    style=(format!("width: {:.1}%", g.percent)) { }
                            }
                            span class="gauge-value" { (format!("{:.1}%", g.percent)) }
                            span class=(format!("gauge-status {}", g.status.class())) {
                                (g.status.label())
                            }
                        }
                    }
                    div class="gauge gauge-load" {
                        span class="gauge-label" { "Load" }
                        span class="gauge-value" { (format!("{:.2}", load)) }
                    }
                }
            }
        }
    }
}

/// Resource grid: disk capacity (per mount) and network (per interface).
pub fn resource_grid_section(model: &DashboardModel) -> Markup {
    html! {
        section class="resource-grid" {
            (disk_section(&model.disk_mounts))
            (network_section(&model.network))
        }
    }
}

/// Per-mount disk capacity rows with explicit utilization status.
pub fn disk_section(mounts: &[DiskMountWidget]) -> Markup {
    html! {
        section id="disk-capacity" class="disk-capacity" {
            h2 { "Disk capacity" }
            @if mounts.is_empty() {
                (EmptyState::new("No disk data", "Disk utilization is unavailable for your scope.").render())
            } @else {
                ul {
                    @for m in mounts {
                        li {
                            span class="mount" { (m.mount) }
                            div class="gauge-bar" {
                                div class=(format!("gauge-fill {}", m.status.class()))
                                    style=(format!("width: {:.1}%", m.percent)) { }
                            }
                            span class=(format!("gauge-value {}", m.status.class())) {
                                (format!("{:.1}%", m.percent))
                            }
                            span class=(format!("disk-status {}", m.status.class())) {
                                (m.status.label())
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Per-interface network throughput rows plus a totals summary row.
/// Totals come from [`traffic_summary`] over the same rows.
pub fn network_section(interfaces: &[NetworkWidget]) -> Markup {
    let total = traffic_summary(interfaces);
    html! {
        section id="network-throughput" class="network-throughput" {
            h2 { "Network" }
            @if interfaces.is_empty() {
                (EmptyState::new("No network data", "Network throughput is unavailable for your scope.").render())
            } @else {
                table class="network-table" {
                    thead {
                        tr { th { "Interface" } th { "In" } th { "Out" } }
                    }
                    tbody {
                        @for n in interfaces {
                            tr {
                                td { (n.interface) }
                                td { (format_bytes_per_sec(n.rx)) }
                                td { (format_bytes_per_sec(n.tx)) }
                            }
                        }
                    }
                }
                p {
                    "Total " (total.interfaces) " interface(s): in "
                    (format_bytes_per_sec(total.total_rx)) " · out "
                    (format_bytes_per_sec(total.total_tx)) " · "
                    a href="/monitoring" { "Monitoring" }
                }
            }
        }
    }
}

/// Trend panel reusing the existing monitoring sparkline for recent CPU.
pub fn trend_section(samples: &[MetricSample]) -> Markup {
    html! {
        section id="trend-cpu" class="trend-cpu" {
            h2 { "CPU trend" }
            @if samples.is_empty() {
                (EmptyState::new("No trend data", "CPU history has not been collected yet.").render())
            } @else {
                (sparkline(samples, MetricKind::Cpu))
            }
        }
    }
}

/// Render the six quick-count cards, each linking to its resource page.
/// The FTP card shows an explicit unavailable state on collection failure.
pub fn cards_section(counts: &ResourceCounts) -> Markup {
    let cards = [
        ("/sites", "Sites", counts.sites),
        ("/ftp", "FTP", counts.ftp),
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
                        @if href == "/ftp" {
                            @if let Some(_) = &counts.ftp_error {
                                span class="card-count" { "—" }
                                span class="card-label" { (label) " (Unavailable)" }
                            } @else {
                                span class="card-count" { (count) }
                                span class="card-label" { (label) }
                            }
                        } @else {
                            span class="card-count" { (count) }
                            span class="card-label" { (label) }
                        }
                    }
                }
            }
            @if let Some(msg) = &counts.ftp_error {
                p { "FTP accounts unavailable: " (msg) " " a href="/ftp" { "Retry" } }
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
                (EmptyState::new("No alerts", "Alert events appear here when thresholds are crossed.").render())
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

/// Render contextual quick actions linking to existing workflows.
pub fn quick_actions_section(actions: &[QuickAction]) -> Markup {
    html! {
        section id="quick-actions" class="quick-actions" {
            h2 { "Quick actions" }
            div class="action-grid" {
                @for action in actions {
                    a class="btn" href=(action.href) { (action.label) }
                }
            }
        }
    }
}

/// Build the dashboard model from existing services, isolating each region's
/// failure to its own error/empty state. `pinned` carries the
/// Display-on-dashboard toggle selection (entry ids); it only filters the
/// rendered shortcuts.
pub async fn build_dashboard(
    state: &WebState,
    caller: &User,
    pinned: &HashSet<String>,
) -> DashboardModel {
    let is_owner = caller.role() == Role::Owner;
    let (gauges, load, disk_mounts, monitoring_error, last_updated) =
        match state.monitoring.snapshot_now().await {
            Ok(snapshot) => {
                let (g, l) = gauges_from_snapshot(&snapshot);
                let mounts = snapshot
                    .disk
                    .iter()
                    .map(|d| {
                        let pct = clamp_percent(d.percent);
                        DiskMountWidget {
                            mount: d.mount.clone(),
                            percent: pct,
                            status: status_for_percent(pct),
                        }
                    })
                    .collect();
                (g, l, mounts, None, snapshot.timestamp)
            }
            Err(e) => (vec![], 0.0, vec![], Some(format!("{e}")), Utc::now()),
        };

    let network = match state.monitoring.snapshot_now().await {
        Ok(snapshot) => snapshot
            .network
            .iter()
            .map(|n| NetworkWidget {
                interface: n.interface.clone(),
                rx: n.rx_bytes_per_sec,
                tx: n.tx_bytes_per_sec,
            })
            .collect(),
        Err(_) => vec![],
    };

    let counts = collect_counts(state, caller).await;
    let alerts = collect_alerts(state).await;
    let attention = build_attention(state, caller, is_owner).await;
    let quick_actions = quick_actions_for(is_owner);
    let panel_shortcuts = panel_shortcuts_for(is_owner);
    let security_risk = if is_owner {
        match state
            .operator_security
            .attention_counts(caller, Utc::now())
            .await
        {
            Ok(open) => security_risk_card(&open),
            Err(e) => SecurityRiskCard {
                open: 0,
                error: Some(format!("Security findings unavailable: {e}")),
            },
        }
    } else {
        SecurityRiskCard {
            open: 0,
            error: None,
        }
    };
    let (software, software_error) = if is_owner {
        match state
            .software_center
            .search(caller.role(), CatalogQuery::default())
            .await
        {
            Ok(page) => {
                let entries: Vec<(String, String)> = page
                    .hits
                    .iter()
                    .map(|hit| (hit.id.clone(), hit.name.clone()))
                    .collect();
                (software_shortcuts(&entries, pinned), None)
            }
            Err(e) => (vec![], Some(format!("Software catalog unavailable: {e}"))),
        }
    } else {
        (vec![], None)
    };
    let quota = match state.container_runtime.get_quota(caller, caller.id()).await {
        Ok(effective) => quota_summary_from(&effective.quota),
        Err(e) => QuotaSummary {
            max_concurrent: 0,
            cpu_pct_max: 0,
            memory_bytes_max: 0,
            egress_bytes_per_month: 0,
            error: Some(format!("Quota unavailable: {e}")),
        },
    };
    let cpu_trend = state
        .monitoring
        .history(MetricKind::Cpu, Utc::now() - Duration::hours(1))
        .await
        .unwrap_or_default();

    let traffic = traffic_summary(&network);
    DashboardModel {
        server_name: server_name(state),
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
        panel_version: state.installation.version,
        last_updated,
        gauges,
        load,
        disk_mounts,
        network,
        counts,
        alerts,
        attention,
        quick_actions,
        monitoring_error,
        cpu_trend,
        is_owner,
        security_risk,
        software,
        software_error,
        traffic,
        panel_shortcuts,
        quota,
    }
}

/// Build the attention queue from host-level (owner) and backup (role-scoped)
/// services. Failures are skipped so a single unavailable service never blanks
/// the queue.
async fn build_attention(state: &WebState, caller: &User, is_owner: bool) -> Vec<AttentionItem> {
    let mut items = Vec::new();

    if is_owner {
        if let Ok(blocks) = state.security.blocks().await {
            let active = blocks.iter().filter(|b| b.is_active(Utc::now())).count();
            if active > 0 {
                items.push(AttentionItem {
                    severity: Severity::Warning,
                    label: format!("{active} active login block(s)"),
                    detail: "Review host firewall and login throttling.".into(),
                    href: Some("/security".into()),
                });
            }
        }
        if let Ok(counts) = state
            .operator_security
            .attention_counts(caller, Utc::now())
            .await
        {
            let open: usize = counts.values().sum();
            if open > 0 {
                items.push(AttentionItem {
                    severity: Severity::Critical,
                    label: format!("{open} open security finding(s)"),
                    detail: "Open the findings queue to preview, remediate, or suppress.".into(),
                    href: Some("/security/findings".into()),
                });
            }
        }
        if let Ok(services) = state.system_services.inventory().await {
            for svc in services {
                if svc.status.active_state != "active" {
                    items.push(AttentionItem {
                        severity: Severity::Critical,
                        label: format!("{} not running", svc.descriptor.display_name()),
                        detail: format!("systemd state: {}", svc.status.active_state),
                        href: Some("/system-services".into()),
                    });
                }
            }
        }
    }

    if let Ok(runs) = state.backups.runs(caller.id(), is_owner).await {
        let failed = runs
            .iter()
            .filter(|r| r.state() == BackupRunState::Failed)
            .count();
        if failed > 0 {
            items.push(AttentionItem {
                severity: Severity::Critical,
                label: format!("{failed} failed backup run(s)"),
                detail: "Open backups to retry or restore.".into(),
                href: Some("/backups".into()),
            });
        }
    }

    items
}

/// Quick actions appropriate to the caller's role.
pub(crate) fn quick_actions_for(is_owner: bool) -> Vec<QuickAction> {
    let mut actions = vec![
        QuickAction {
            label: "Manage sites",
            href: "/sites",
        },
        QuickAction {
            label: "Open files",
            href: "/files",
        },
        QuickAction {
            label: "Issue / renew SSL",
            href: "/ssl",
        },
        QuickAction {
            label: "View monitoring",
            href: "/monitoring",
        },
    ];
    if is_owner {
        actions.push(QuickAction {
            label: "Create backup",
            href: "/backups",
        });
        actions.push(QuickAction {
            label: "Security",
            href: "/security",
        });
    }
    actions
}

/// Derive the server display name from installed metadata (hostname-less by
/// design; falls back to the panel identity).
fn server_name(state: &WebState) -> String {
    format!("OpenPanel {}", state.installation.version)
}

/// Count sites, FTP accounts (across caller-visible sites), databases,
/// files (top-level entries across site roots), SSL certs, and users.
/// FTP failures yield zero with an error state, never a silent zero.
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
    let (ftp, ftp_error) = match state.sites.list_sites(caller).await {
        Err(e) => (0, Some(format!("FTP unavailable: {e}"))),
        Ok(site_list) => {
            let mut per_site = Vec::with_capacity(site_list.len());
            for site in &site_list {
                per_site.push(
                    state
                        .ftp
                        .list(caller, site.id())
                        .await
                        .map(|v| v.len())
                        .map_err(|e| e.to_string()),
                );
            }
            count_ftp_accounts(&per_site)
        }
    };
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
        ftp,
        ftp_error,
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

pub(crate) fn alert_from_event(event: &AuditEvent) -> AlertView {
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

/// Build CPU/Memory/Disk gauges and the load figure from a snapshot.
pub(crate) fn gauges_from_snapshot(snapshot: &SystemSnapshot) -> (Vec<GaugeWidget>, f64) {
    let cpu = clamp_percent(snapshot.cpu);
    let memory = clamp_percent(snapshot.memory);
    let disk = snapshot.disk.iter().map(|d| d.percent).fold(0.0, f64::max);
    let disk = clamp_percent(disk);
    let gauges = vec![
        GaugeWidget {
            label: "CPU",
            percent: cpu,
            status: status_for_percent(cpu),
        },
        GaugeWidget {
            label: "Memory",
            percent: memory,
            status: status_for_percent(memory),
        },
        GaugeWidget {
            label: "Disk",
            percent: disk,
            status: status_for_percent(disk),
        },
    ];
    (gauges, snapshot.load.max(0.0))
}

/// Map a utilization percentage to a status.
pub(crate) fn status_for_percent(pct: f64) -> ResourceStatus {
    if !pct.is_finite() {
        ResourceStatus::Unknown
    } else if pct >= DEGRADED_THRESHOLD {
        ResourceStatus::Degraded
    } else {
        ResourceStatus::Healthy
    }
}

/// Whether a snapshot timestamp is old enough to be flagged stale.
pub fn is_stale(ts: chrono::DateTime<Utc>) -> bool {
    Utc::now() - ts > STALE_THRESHOLD
}

/// Clamp a percentage into `[0, 100]`, treating non-finite as zero.
fn clamp_percent(v: f64) -> f64 {
    if !v.is_finite() {
        0.0
    } else {
        v.clamp(0.0, 100.0)
    }
}

/// Human-readable bytes/sec, e.g. `1.5 MB/s`.
fn format_bytes_per_sec(bytes: u64) -> String {
    const UNITS: &[&str] = &["B/s", "KB/s", "MB/s", "GB/s", "TB/s"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B/s")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests;
