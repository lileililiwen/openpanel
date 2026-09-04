//! Tests for the operations dashboard widgets and model builder.

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome};
use openpanel_domain::{DiskReading, SystemSnapshot};

use crate::dashboard::{
    AlertView, AttentionItem, DashboardModel, DiskMountWidget, NetworkWidget, ResourceCounts,
    ResourceStatus, Severity, alert_from_event, alerts_section, attention_section, cards_section,
    disk_section, gauges_from_snapshot, gauges_region, header_section, is_stale, network_section,
    quick_actions_for, status_for_percent,
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
