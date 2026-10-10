//! Dashboard discoverability widgets for `repair-home-discoverability`.
//!
//! Pure view-model projections plus their render functions live here so
//! `dashboard/mod.rs` (composition root of the page) stays under the
//! `file-length` hard limit. Everything is re-exported through
//! `crate::dashboard`, so paths and the view/tests split are unchanged.

use std::collections::{HashMap, HashSet};

use maud::{Markup, html};
use openpanel_domain::ContainerQuota;

use super::{NetworkWidget, QuickAction};
use crate::ui_states::{EmptyState, ErrorState};

/// Security-risk card: open control-plane findings with a fix link.
#[derive(Debug, Clone, PartialEq)]
pub struct SecurityRiskCard {
    /// Total open findings across severities.
    pub open: usize,
    /// Set when the queue could not be read; renders error, never zero.
    pub error: Option<String>,
}

/// One pinned Software Center shortcut (Display-on-dashboard subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareShortcut {
    /// Catalog entry id.
    pub id: String,
    /// Catalog entry name.
    pub name: String,
}

/// Traffic totals aggregating the per-interface network rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrafficSummary {
    /// Sum of inbound bytes/sec.
    pub total_rx: u64,
    /// Sum of outbound bytes/sec.
    pub total_tx: u64,
    /// Number of interfaces aggregated.
    pub interfaces: usize,
}

/// Per-caller quota summary projected from the container quota surface:
/// QPS ≈ max concurrent, bandwidth/traffic ≈ monthly egress cap, disk ≈
/// memory cap. Role-scoped: only the caller's own quota is projected.
#[derive(Debug, Clone, PartialEq)]
pub struct QuotaSummary {
    /// Max running containers (parallelism / QPS proxy).
    pub max_concurrent: u32,
    /// Max per-container CPU percent (rate proxy, clamped to 100).
    pub cpu_pct_max: u8,
    /// Max per-container memory bytes (disk proxy).
    pub memory_bytes_max: u64,
    /// Max monthly egress bytes (bandwidth/traffic).
    pub egress_bytes_per_month: u64,
    /// Set when the quota could not be read.
    pub error: Option<String>,
}

/// Dashboard deep-links added by this change. Every entry MUST resolve via
/// `capability_registry::find_by_route` (see `prop_dashboard_links_resolve`).
pub fn dashboard_links() -> Vec<&'static str> {
    vec![
        "/",
        "/sites",
        "/ftp",
        "/databases",
        "/files",
        "/ssl",
        "/users",
        "/security",
        "/security/findings",
        "/monitoring",
        "/software",
        "/settings",
        "/services",
        "/backups",
        "/container/quota",
    ]
}

/// Panel Update/Fix/Restart shortcuts as deep-links to existing mounted
/// routes (no self-update engine). Owner-only; every href resolves via
/// `find_by_route`.
pub fn panel_shortcuts_for(is_owner: bool) -> Vec<QuickAction> {
    if !is_owner {
        return vec![];
    }
    vec![
        QuickAction {
            label: "Update",
            href: "/software",
        },
        QuickAction {
            label: "Fix",
            href: "/settings",
        },
        QuickAction {
            label: "Restart",
            href: "/services",
        },
    ]
}

/// Sum per-site FTP account counts scoped to the caller. Any single
/// failure yields zero with an error state, never a silent/partial zero.
pub fn count_ftp_accounts(results: &[Result<usize, String>]) -> (usize, Option<String>) {
    let mut total: usize = 0;
    for result in results {
        match result {
            Ok(n) => total = total.saturating_add(*n),
            Err(e) => return (0, Some(format!("FTP unavailable: {e}"))),
        }
    }
    (total, None)
}

/// Project open-findings counts into the risk card. Count only; finding
/// evidence stays redacted by construction.
pub fn security_risk_card(counts: &HashMap<String, usize>) -> SecurityRiskCard {
    SecurityRiskCard {
        open: counts.values().sum(),
        error: None,
    }
}

/// Filter catalog `(id, name)` hits by the Display-on-dashboard toggle. An
/// empty toggle shows the catalog's first page in order (max 6); a
/// non-empty toggle shows only the pinned subset. Pure filter: the
/// catalog is never mutated.
pub fn software_shortcuts(
    entries: &[(String, String)],
    pinned: &HashSet<String>,
) -> Vec<SoftwareShortcut> {
    entries
        .iter()
        .filter(|(id, _)| pinned.is_empty() || pinned.contains(id))
        .take(6)
        .map(|(id, name)| SoftwareShortcut {
            id: id.clone(),
            name: name.clone(),
        })
        .collect()
}

/// Aggregate per-interface network rows into traffic totals (no new
/// monitoring collection; saturating sums over existing rows).
pub fn traffic_summary(network: &[NetworkWidget]) -> TrafficSummary {
    TrafficSummary {
        total_rx: network.iter().map(|n| n.rx).fold(0u64, u64::saturating_add),
        total_tx: network.iter().map(|n| n.tx).fold(0u64, u64::saturating_add),
        interfaces: network.len(),
    }
}

/// Project the caller's effective container quota onto the summary axes,
/// clamping the rate to 100. Takes only the caller's own quota, so no
/// cross-user data can leak.
pub fn quota_summary_from(quota: &ContainerQuota) -> QuotaSummary {
    QuotaSummary {
        max_concurrent: quota.max_concurrent,
        cpu_pct_max: quota.cpu_pct_max.min(100),
        memory_bytes_max: quota.memory_bytes_max,
        egress_bytes_per_month: quota.egress_bytes_per_month,
        error: None,
    }
}

/// Security-risk card projecting open control-plane findings with a
/// one-click fix link. Hidden for non-owners; failures render an error
/// state, never a healthy zero.
pub fn security_risk_section(card: &SecurityRiskCard, is_owner: bool) -> Markup {
    if !is_owner {
        return html! {};
    }
    html! {
        section id="security-risk" class="cards" aria-live="polite" {
            h2 { "Security risk" }
            @if let Some(msg) = &card.error {
                (ErrorState::new("Security findings unavailable", msg, "/security/findings").render())
            } @else if card.open == 0 {
                (EmptyState::new("No open findings", "The control plane reports no open findings.").render())
            } @else {
                div class="card-grid" {
                    a class="card" href="/security/findings" {
                        span class="card-count" { (card.open) }
                        span class="card-label" { "Open findings — Review and fix" }
                    }
                }
            }
        }
    }
}

/// Software shortcuts filtered by the Display-on-dashboard toggle. Each pin
/// link round-trips through `GET /dashboard?pin=<id>` (no JavaScript, no
/// catalog mutation); `All` clears the filter.
pub fn software_section(shortcuts: &[SoftwareShortcut], error: Option<String>) -> Markup {
    html! {
        section id="software-shortcuts" class="cards" aria-live="polite" {
            h2 { "Software" }
            @if let Some(msg) = &error {
                (ErrorState::new("Software catalog unavailable", msg, "/software").render())
            } @else if shortcuts.is_empty() {
                (EmptyState::new("No software shortcuts", "Pin entries with Display-on-dashboard to see them here.").with_cta("/software", "Open Software Center").render())
            } @else {
                div class="card-grid" {
                    @for s in shortcuts {
                        a class="card" href="/software" {
                            span class="card-label" { (s.name) }
                        }
                    }
                }
                p {
                    @for s in shortcuts {
                        a class="btn" href=(format!("/dashboard?pin={}", s.id)) { (s.name) }
                    }
                    a class="btn" href="/dashboard" { "All" }
                }
            }
        }
    }
}

/// Panel Update/Fix/Restart shortcuts. Owner-only; renders nothing when
/// the action list is empty.
pub fn panel_shortcuts_section(actions: &[QuickAction]) -> Markup {
    if actions.is_empty() {
        return html! {};
    }
    html! {
        section id="panel-shortcuts" class="quick-actions" {
            h2 { "Panel" }
            div class="action-grid" {
                @for action in actions {
                    a class="btn" href=(action.href) { (action.label) }
                }
            }
        }
    }
}

/// Caller-scoped quota summary (QPS / bandwidth / traffic / disk) with a
/// drill-down to the quota workflow. Failures render an error state.
pub fn quota_section(summary: &QuotaSummary) -> Markup {
    html! {
        section id="quota-summary" class="cards" aria-live="polite" {
            h2 { "Quota" }
            @if let Some(msg) = &summary.error {
                (ErrorState::new("Quota unavailable", msg, "/container/quota").render())
            } @else {
                table class="table" {
                    tbody {
                        tr { td { "QPS (max concurrent)" } td { (summary.max_concurrent) } }
                        tr { td { "Bandwidth" } td { (summary.egress_bytes_per_month) " bytes/month" } }
                        tr { td { "Traffic" } td { (summary.egress_bytes_per_month) " bytes/month" } }
                        tr { td { "Disk" } td { (summary.memory_bytes_max) " bytes" } }
                    }
                }
                p { a class="btn" href="/container/quota" { "Manage quota" } }
            }
        }
    }
}
