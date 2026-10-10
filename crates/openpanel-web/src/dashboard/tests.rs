//! Tests for the operations dashboard widgets and model builder.

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome};
use openpanel_domain::{DiskReading, SystemSnapshot};

use crate::dashboard::{
    AlertView, AttentionItem, DashboardModel, DiskMountWidget, NetworkWidget, QuotaSummary,
    ResourceCounts, ResourceStatus, SecurityRiskCard, Severity, TrafficSummary, alert_from_event,
    alerts_section, attention_section, cards_section, count_ftp_accounts, dashboard_content,
    dashboard_links, disk_section, gauges_from_snapshot, gauges_region, header_section, is_stale,
    network_section, panel_shortcuts_for, quick_actions_for, quota_section, quota_summary_from,
    security_risk_card, security_risk_section, software_section, software_shortcuts,
    status_for_percent, traffic_summary,
};

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

fn model_with(error: Option<String>, stale: bool) -> DashboardModel {
    let ts = if stale {
        Utc::now() - Duration::minutes(10)
    } else {
        Utc::now()
    };
    DashboardModel {
        server_name: "OpenPanel x".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        panel_version: "1.0.0",
        last_updated: ts,
        gauges: vec![],
        load: 0.0,
        disk_mounts: vec![],
        network: vec![],
        counts: ResourceCounts {
            sites: 0,
            ftp: 0,
            ftp_error: None,
            databases: 0,
            files: 0,
            ssl: 0,
            users: 0,
        },
        alerts: vec![],
        attention: vec![],
        quick_actions: vec![],
        monitoring_error: error,
        cpu_trend: vec![],
        is_owner: true,
        security_risk: SecurityRiskCard {
            open: 0,
            error: None,
        },
        software: vec![],
        software_error: None,
        traffic: TrafficSummary {
            total_rx: 0,
            total_tx: 0,
            interfaces: 0,
        },
        panel_shortcuts: vec![],
        quota: QuotaSummary {
            max_concurrent: 0,
            cpu_pct_max: 0,
            memory_bytes_max: 0,
            egress_bytes_per_month: 0,
            error: None,
        },
    }
}

// --- gauges / status -------------------------------------------------

#[test]
fn gauges_render_all_values_with_status() {
    let (gauges, load) = gauges_from_snapshot(&snapshot());
    let out = gauges_region(&gauges, load, None).into_string();
    assert!(out.contains("42.3%"), "cpu gauge: {out}");
    assert!(out.contains("57.8%"), "memory gauge: {out}");
    assert!(out.contains("81.4%"), "disk gauge: {out}");
    assert!(out.contains("1.25"), "load figure: {out}");
    assert!(out.contains("id=\"host-gauges\""), "region id present");
    assert!(
        out.contains("hx-trigger=\"every 30s\""),
        "auto-refresh trigger"
    );
    assert!(out.contains("aria-live=\"polite\""), "live region");
    assert!(out.contains("Healthy"), "text status present");
}

#[test]
fn degraded_gauge_renders_degraded_status() {
    let mut g = gauges_from_snapshot(&snapshot()).0;
    g[0].percent = 92.0;
    g[0].status = ResourceStatus::Degraded;
    let out = gauges_region(&g, 0.0, None).into_string();
    assert!(out.contains("Degraded"), "degraded label: {out}");
    assert!(out.contains("op-status-degraded"), "degraded class");
}

#[test]
fn failed_collection_renders_error_not_zero() {
    let out = gauges_region(&[], 0.0, Some("boom".into())).into_string();
    assert!(out.contains("op-error-state"), "error state: {out}");
    assert!(out.contains("boom"), "error message surfaced: {out}");
    assert!(!out.contains("42.3%"), "no silent healthy value");
}

// --- disk / network --------------------------------------------------

#[test]
fn disk_section_renders_mounts_and_status() {
    let mounts = vec![
        DiskMountWidget {
            mount: "/".into(),
            percent: 90.0,
            status: ResourceStatus::Degraded,
        },
        DiskMountWidget {
            mount: "/var".into(),
            percent: 30.0,
            status: ResourceStatus::Healthy,
        },
    ];
    let out = disk_section(&mounts).into_string();
    assert!(out.contains("/"), "root mount");
    assert!(out.contains("/var"), "var mount");
    assert!(out.contains("90.0%"), "root percent");
    assert!(out.contains("Degraded"), "degraded label");
}

#[test]
fn network_section_formats_throughput() {
    let net = vec![NetworkWidget {
        interface: "eth0".into(),
        rx: 1536,
        tx: 1_048_576,
    }];
    let out = network_section(&net).into_string();
    assert!(out.contains("eth0"), "interface");
    assert!(out.contains("1.0 MB/s"), "tx formatted: {out}");
}

// --- header / stale --------------------------------------------------

#[test]
fn header_shows_identity_and_fresh_state() {
    let out = header_section(&model_with(None, false)).into_string();
    assert!(out.contains("OpenPanel x"), "server name");
    assert!(out.contains("linux"), "os");
    assert!(out.contains("x86_64"), "arch");
    assert!(out.contains("1.0.0"), "version");
    assert!(out.contains("Last updated"), "timestamp label");
    assert!(!out.contains("stale-flag"), "not stale");
}

#[test]
fn header_flags_stale_snapshot() {
    let out = header_section(&model_with(None, true)).into_string();
    assert!(out.contains("stale-flag"), "stale flag: {out}");
    assert!(out.contains("op-status-stale"), "stale class");
}

// --- attention queue -------------------------------------------------

#[test]
fn attention_queue_renders_items() {
    let items = vec![
        AttentionItem {
            severity: Severity::Critical,
            label: "nginx not running".into(),
            detail: "systemd state: failed".into(),
            href: Some("/system-services".into()),
        },
        AttentionItem {
            severity: Severity::Warning,
            label: "1 active login block".into(),
            detail: "Review host firewall.".into(),
            href: Some("/security".into()),
        },
    ];
    let out = attention_section(&items).into_string();
    assert!(out.contains("nginx not running"), "service item");
    assert!(out.contains("/system-services"), "service link");
    assert!(out.contains("op-attn-critical"), "critical class");
    assert!(out.contains("op-attn-warning"), "warning class");
    assert!(out.contains("Review"), "remediation link");
}

#[test]
fn empty_attention_renders_all_clear() {
    let out = attention_section(&[]).into_string();
    assert!(out.contains("Nothing needs attention"), "all-clear: {out}");
    assert!(out.contains("op-empty-state"), "empty state class");
}

// --- quick actions / scope ------------------------------------------

#[test]
fn user_quick_actions_omit_owner_only() {
    let actions = quick_actions_for(false);
    assert!(
        actions
            .iter()
            .all(|a| a.href != "/backups" && a.href != "/security"),
        "user must not see owner-only actions: {actions:?}"
    );
}

#[test]
fn owner_quick_actions_include_backup_and_security() {
    let actions = quick_actions_for(true);
    let hrefs: Vec<&str> = actions.iter().map(|a| a.href).collect();
    assert!(hrefs.contains(&"/backups"), "backup action present");
    assert!(hrefs.contains(&"/security"), "security action present");
}

#[test]
fn cards_render_counts_and_links() {
    let counts = ResourceCounts {
        sites: 3,
        ftp: 4,
        ftp_error: None,
        databases: 2,
        files: 14,
        ssl: 1,
        users: 5,
    };
    let out = cards_section(&counts).into_string();
    for (href, label, count) in [
        ("/sites", "Sites", 3),
        ("/ftp", "FTP", 4),
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
fn alert_from_event_extracts_metadata() {
    let event = AuditEvent::new("monitoring", AuditAction::AlertFired, AuditOutcome::Success)
        .target("Cpu")
        .metadata(serde_json::json!({ "value": 88.0, "threshold": 80.0 }));
    let view = alert_from_event(&event);
    assert_eq!(view.metric, "Cpu");
    assert_eq!(view.value, 88.0);
    assert_eq!(view.threshold, 80.0);
}

#[test]
fn status_for_percent_buckets() {
    assert_eq!(status_for_percent(10.0), ResourceStatus::Healthy);
    assert_eq!(status_for_percent(90.0), ResourceStatus::Degraded);
    assert_eq!(status_for_percent(f64::NAN), ResourceStatus::Unknown);
}

#[test]
fn is_stale_detects_old_snapshot() {
    assert!(is_stale(Utc::now() - Duration::minutes(10)));
    assert!(!is_stale(Utc::now() - Duration::minutes(1)));
}

// --- repair-home-discoverability (2026-10-10) ----------------------------

use std::collections::{HashMap, HashSet};

#[test]
fn ftp_count_sums_caller_scoped_sites() {
    let (count, error) = count_ftp_accounts(&[Ok(2), Ok(3)]);
    assert_eq!(count, 5);
    assert!(error.is_none(), "no error on success");
}

#[test]
fn ftp_count_failure_yields_zero_with_error_state() {
    let (count, error) = count_ftp_accounts(&[Ok(2), Err("db down".to_string())]);
    assert_eq!(count, 0, "failure never reports a partial count");
    let error = error.expect("error state, not silent zero");
    assert!(error.contains("db down"));
    let counts = ResourceCounts {
        sites: 1,
        ftp: count,
        ftp_error: Some(error),
        databases: 0,
        files: 0,
        ssl: 0,
        users: 0,
    };
    let out = cards_section(&counts).into_string();
    assert!(out.contains("href=\"/ftp\""), "ftp deep-link: {out}");
    assert!(
        out.contains("Unavailable"),
        "error state, never zero: {out}"
    );
}

#[test]
fn security_risk_card_links_open_findings() {
    let counts: HashMap<String, usize> =
        HashMap::from([("critical".to_string(), 2), ("warning".to_string(), 1)]);
    let card = security_risk_card(&counts);
    assert_eq!(card.open, 3);
    assert!(card.error.is_none());
    let out = security_risk_section(&card, true).into_string();
    assert!(
        out.contains("href=\"/security/findings\""),
        "fix link: {out}"
    );
    assert!(out.contains('3'), "open count: {out}");
}

#[test]
fn security_risk_card_all_clear_redacted_and_role_gated() {
    let card = security_risk_card(&HashMap::new());
    assert_eq!(card.open, 0);
    let out = security_risk_section(&card, true).into_string();
    assert!(out.contains("No open findings"), "all-clear: {out}");
    assert!(!out.contains("password"), "no secret material: {out}");
    assert!(!out.contains("BEGIN"), "no key material: {out}");
    let hidden = security_risk_section(&card, false).into_string();
    assert!(
        !hidden.contains("/security/findings"),
        "owner-only card hidden for users"
    );
}

#[test]
fn security_risk_card_error_renders_unavailable() {
    let card = SecurityRiskCard {
        open: 0,
        error: Some("control plane down".into()),
    };
    let out = security_risk_section(&card, true).into_string();
    assert!(out.contains("op-error-state"), "error state: {out}");
    assert!(out.contains("control plane down"), "cause surfaced: {out}");
}

#[test]
fn software_shortcuts_filter_by_display_toggle() {
    let entries = vec![
        ("nginx".to_string(), "nginx".to_string()),
        ("redis".to_string(), "Redis".to_string()),
    ];
    let pinned: HashSet<String> = HashSet::from(["redis".to_string()]);
    let filtered = software_shortcuts(&entries, &pinned);
    assert_eq!(filtered.len(), 1, "toggle filters to pinned subset");
    assert_eq!(filtered[0].id, "redis");
    let unpinned: HashSet<String> = HashSet::new();
    let all = software_shortcuts(&entries, &unpinned);
    assert_eq!(all.len(), 2, "empty toggle shows catalog order");
    let out = software_section(&[], None).into_string();
    assert!(
        out.contains("op-empty-state"),
        "empty toggle renders empty state: {out}"
    );
    let err = software_section(&[], Some("catalog down".to_string())).into_string();
    assert!(
        err.contains("op-error-state"),
        "failure renders error: {err}"
    );
}

#[test]
fn traffic_summary_aggregates_network_rows() {
    let net = vec![
        NetworkWidget {
            interface: "eth0".into(),
            rx: 1000,
            tx: 2000,
        },
        NetworkWidget {
            interface: "lo".into(),
            rx: 500,
            tx: 500,
        },
    ];
    let summary = traffic_summary(&net);
    assert_eq!(summary.total_rx, 1500);
    assert_eq!(summary.total_tx, 2500);
    assert_eq!(summary.interfaces, 2);
    let empty = traffic_summary(&[]);
    assert_eq!(
        (empty.total_rx, empty.total_tx, empty.interfaces),
        (0, 0, 0)
    );
    let out = network_section(&net).into_string();
    assert!(out.contains("eth0"), "rows preserved: {out}");
    assert!(out.contains("Total"), "summary row: {out}");
}

#[test]
fn quota_summary_projects_axes_with_clamp() {
    let quota = openpanel_domain::ContainerQuota::default_for(uuid::Uuid::new_v4());
    let summary = quota_summary_from(&quota);
    assert_eq!(summary.max_concurrent, quota.max_concurrent, "QPS axis");
    assert_eq!(
        summary.egress_bytes_per_month, quota.egress_bytes_per_month,
        "bandwidth/traffic axis"
    );
    assert_eq!(
        summary.memory_bytes_max, quota.memory_bytes_max,
        "disk axis"
    );
    assert!(summary.error.is_none());
    let mut over = quota.clone();
    over.cpu_pct_max = 200;
    let clamped = quota_summary_from(&over);
    assert!(clamped.cpu_pct_max <= 100, "rate clamped");
    let out = quota_section(&summary).into_string();
    assert!(out.contains("/container/quota"), "quota drill-down: {out}");
    assert!(out.contains("Bandwidth"), "bandwidth row: {out}");
}

#[test]
fn panel_shortcuts_resolve_and_gate_by_role() {
    let owner = panel_shortcuts_for(true);
    assert_eq!(owner.len(), 3, "Update/Fix/Restart");
    for action in &owner {
        assert!(
            crate::capability_registry::find_by_route(action.href).is_some(),
            "shortcut {} resolves to registry",
            action.href
        );
    }
    assert!(
        panel_shortcuts_for(false).is_empty(),
        "owner-only for users"
    );
}

#[test]
fn dashboard_content_renders_discoverability_widgets() {
    let mut model = model_with(None, false);
    model.counts.ftp = 2;
    model.security_risk = SecurityRiskCard {
        open: 1,
        error: None,
    };
    let out = dashboard_content(&model).into_string();
    assert_eq!(out.matches("<h1>").count(), 1, "single h1: {out}");
    assert!(out.contains("href=\"/ftp\""), "ftp card: {out}");
    assert!(out.contains("/security/findings"), "risk fix link: {out}");
    assert!(out.contains("/container/quota"), "quota summary: {out}");
}

mod prop {
    use super::*;

    #[test]
    fn prop_dashboard_links_resolve() {
        for href in dashboard_links() {
            assert!(
                crate::capability_registry::find_by_route(href).is_some(),
                "dashboard href `{href}` has no registry entry"
            );
        }
    }

    #[test]
    fn prop_dashboard_never_silent_zero() {
        let (_count, error) = count_ftp_accounts(&[Err("down".to_string())]);
        let counts = ResourceCounts {
            sites: 0,
            ftp: 0,
            ftp_error: error,
            databases: 0,
            files: 0,
            ssl: 0,
            users: 0,
        };
        let cards = cards_section(&counts).into_string();
        assert!(
            cards.contains("FTP (Unavailable)") && cards.contains(">—<"),
            "ftp failure is unavailable, never healthy zero: {cards}"
        );
        let gauges = gauges_region(&[], 0.0, Some("down".into())).into_string();
        assert!(
            gauges.contains("op-error-state"),
            "monitoring failure: {gauges}"
        );
        let risk = security_risk_section(
            &SecurityRiskCard {
                open: 0,
                error: Some("down".into()),
            },
            true,
        )
        .into_string();
        assert!(risk.contains("op-error-state"), "risk failure: {risk}");
        let quota = quota_section(&QuotaSummary {
            max_concurrent: 0,
            cpu_pct_max: 0,
            memory_bytes_max: 0,
            egress_bytes_per_month: 0,
            error: Some("down".into()),
        })
        .into_string();
        assert!(quota.contains("op-error-state"), "quota failure: {quota}");
    }
}
