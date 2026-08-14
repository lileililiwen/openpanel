//! Monitoring application service: use cases for the time series.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    RepoError,
    monitoring::{
        Alert, MetricKind, MetricSample, MonitoringError, SnapshotRepository, SystemSnapshot,
    },
};

use super::{AlertEvaluator, Collector};
use crate::notifications::NotificationService;

/// Application service orchestrating metric collection, persistence,
/// history, retention, and alert evaluation.
pub struct MonitoringService {
    repo: Arc<dyn SnapshotRepository>,
    collector: Arc<Mutex<Box<dyn Collector>>>,
    audit: Arc<dyn AuditService>,
    evaluator: Arc<Mutex<AlertEvaluator>>,
    retention_days: u64,
    notifications: std::sync::RwLock<Option<Arc<NotificationService>>>,
}

impl MonitoringService {
    /// Construct the service.
    pub fn new(
        repo: Arc<dyn SnapshotRepository>,
        collector: Arc<Mutex<Box<dyn Collector>>>,
        audit: Arc<dyn AuditService>,
        evaluator: AlertEvaluator,
        retention_days: u64,
    ) -> Self {
        Self {
            repo,
            collector,
            audit,
            evaluator: Arc::new(Mutex::new(evaluator)),
            retention_days,
            notifications: std::sync::RwLock::new(None),
        }
    }

    /// Attach the notification publisher during composition.
    pub fn attach_notifications(&self, service: Arc<NotificationService>) {
        if let Ok(mut notifications) = self.notifications.write() {
            *notifications = Some(service);
        }
    }

    /// Return a clone of the repository handle.
    pub fn repo(&self) -> Arc<dyn SnapshotRepository> {
        self.repo.clone()
    }

    /// Return a clone of the audit handle (used by the `/alerts` route).
    pub fn audit_handle(&self) -> Arc<dyn AuditService> {
        self.audit.clone()
    }

    /// Most recent audit events, newest first, up to `limit`.
    pub async fn audit_events(
        &self,
        limit: i64,
    ) -> Result<Vec<openpanel_core::AuditEvent>, MonitoringError> {
        self.audit
            .recent(limit)
            .await
            .map_err(|e| MonitoringError::Repo(e.to_string()))
    }

    /// Persist every sample of a snapshot (fan-out to one row per
    /// sample). Idempotent per `(ts, kind)` — a duplicate insert is
    /// surfaced as `MonitoringError::Repo`.
    pub async fn record(&self, snapshot: &SystemSnapshot) -> Result<(), MonitoringError> {
        for sample in snapshot.samples() {
            self.repo
                .insert(&sample)
                .await
                .map_err(|e| MonitoringError::Repo(e.into_inner()))?;
        }
        Ok(())
    }

    /// Most recent sample of every kind (one row per kind).
    pub async fn latest(&self) -> Result<Vec<MetricSample>, MonitoringError> {
        self.repo
            .latest()
            .await
            .map_err(|e| MonitoringError::Repo(e.into_inner()))
    }

    /// All samples of one kind newer than `since`, ascending.
    pub async fn history(
        &self,
        kind: MetricKind,
        since: DateTime<Utc>,
    ) -> Result<Vec<MetricSample>, MonitoringError> {
        self.repo
            .history(kind, since)
            .await
            .map_err(|e| MonitoringError::Repo(e.into_inner()))
    }

    /// Delete samples older than `before`; returns rows removed.
    pub async fn prune(&self, before: DateTime<Utc>) -> Result<u64, MonitoringError> {
        self.repo
            .prune(before)
            .await
            .map_err(|e| MonitoringError::Repo(e.into_inner()))
    }

    /// Collect a fresh snapshot from the collector and persist it.
    pub async fn snapshot_now(&self) -> Result<SystemSnapshot, MonitoringError> {
        let snapshot = {
            let mut collector = self
                .collector
                .lock()
                .map_err(|_| MonitoringError::Io("collector mutex poisoned".into()))?;
            collector.snapshot()?
        };
        self.record(&snapshot).await?;
        Ok(snapshot)
    }

    /// Collect a fresh snapshot, persist it, and evaluate alert
    /// thresholds against it. Returns the alerts that fired.
    pub async fn tick(&self) -> Result<Vec<Alert>, MonitoringError> {
        let snapshot = self.snapshot_now().await?;
        if self.retention_days > 0 {
            let before = Utc::now() - Duration::days(self.retention_days as i64);
            let removed = self.prune(before).await?;
            if removed > 0 {
                tracing::debug!(removed, "monitoring: pruned expired samples");
            }
        }
        let alerts = self.evaluate_alerts(&snapshot).await?;
        Ok(alerts)
    }

    /// Evaluate a snapshot against the configured thresholds and
    /// record an `AlertFired` audit event per alert that fires.
    pub async fn evaluate_alerts(
        &self,
        snapshot: &SystemSnapshot,
    ) -> Result<Vec<Alert>, MonitoringError> {
        let fired = {
            let mut evaluator = self
                .evaluator
                .lock()
                .map_err(|_| MonitoringError::Io("alert evaluator mutex poisoned".into()))?;
            evaluator.evaluate(snapshot)
        };
        for alert in &fired {
            self.audit
                .record(
                    AuditEvent::new("monitoring", AuditAction::AlertFired, AuditOutcome::Success)
                        .target(alert.kind.as_str())
                        .metadata(serde_json::json!({
                            "metric": alert.kind.as_str(),
                            "value": alert.value,
                            "threshold": alert.threshold,
                        })),
                )
                .await
                .ok();
            let notifications = self
                .notifications
                .read()
                .ok()
                .and_then(|value| value.clone());
            if let Some(notifications) = notifications {
                let notification_metric = match alert.kind {
                    MetricKind::Cpu => "cpu_percent",
                    MetricKind::Memory => "memory_percent",
                    MetricKind::Disk => "disk_percent",
                    MetricKind::Network => "network_bytes_per_second",
                };
                let event = openpanel_domain::notifications::NotificationEvent::new(
                    uuid::Uuid::new_v4(),
                    openpanel_domain::notifications::EventKind::Alert,
                    format!("{notification_metric} threshold crossed"),
                    openpanel_domain::notifications::Severity::Warning,
                    serde_json::json!({
                        "metric": notification_metric,
                        "value": alert.value,
                        "threshold": alert.threshold,
                    }),
                    Utc::now(),
                );
                if let Ok(event) = event
                    && let Err(error) = notifications.publish(event).await
                {
                    tracing::warn!(error = %error, "monitoring notification publish failed");
                }
            }
        }
        Ok(fired)
    }
}

/// Convenience for mapping repository errors in callers that want
/// `RepoError` → `MonitoringError` without importing the mapping.
pub fn repo_err(e: RepoError) -> MonitoringError {
    MonitoringError::Repo(e.into_inner())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Duration;
    use openpanel_core::AuditAction;
    use openpanel_domain::monitoring::{MetricKind, MetricSample, Unit};
    use openpanel_test_support::{MockAudit, MockSnapshotRepo};

    use super::{
        super::{AlertConfig, AlertEvaluator, Collector},
        MonitoringService,
    };

    /// A collector double that returns a fixed snapshot. The snapshot's
    /// `cpu` is configurable so alert tests can control thresholds.
    struct FixedCollector {
        cpu: f64,
    }

    impl Collector for FixedCollector {
        fn snapshot(
            &mut self,
        ) -> Result<
            openpanel_domain::monitoring::SystemSnapshot,
            openpanel_domain::monitoring::MonitoringError,
        > {
            use openpanel_domain::monitoring::{DiskReading, NetworkReading, SystemSnapshot};
            SystemSnapshot::new(
                chrono::Utc::now(),
                0.5,
                self.cpu,
                40.0,
                vec![DiskReading {
                    mount: "/".into(),
                    percent: 50.0,
                }],
                vec![NetworkReading {
                    interface: "eth0".into(),
                    rx_bytes_per_sec: 100,
                    tx_bytes_per_sec: 50,
                }],
            )
        }
    }

    fn sample(ts: chrono::DateTime<chrono::Utc>, kind: MetricKind, value: f64) -> MetricSample {
        MetricSample {
            kind,
            unit: match kind {
                MetricKind::Cpu | MetricKind::Memory | MetricKind::Disk => Unit::Percent,
                MetricKind::Network => Unit::BytesPerSecond,
            },
            value,
            ts,
        }
    }

    #[tokio::test]
    async fn record_persists_every_sample() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_insert().times(4).returning(|_| Ok(()));
        let audit = MockAudit::stub();
        let collector = Arc::new(std::sync::Mutex::new(
            Box::new(FixedCollector { cpu: 10.0 }) as Box<dyn Collector>,
        ));
        let svc = MonitoringService::new(
            Arc::new(repo),
            collector,
            Arc::new(audit),
            AlertEvaluator::new(&AlertConfig::default()),
            7,
        );
        let now = chrono::Utc::now();
        let snap = openpanel_domain::monitoring::SystemSnapshot::new(
            now,
            0.5,
            10.0,
            40.0,
            vec![openpanel_domain::monitoring::DiskReading {
                mount: "/".into(),
                percent: 50.0,
            }],
            vec![],
        )
        .expect("valid snapshot");
        svc.record(&snap).await.expect("record");
    }

    #[tokio::test]
    async fn latest_returns_newest_sample() {
        let now = chrono::Utc::now();
        let mut repo = MockSnapshotRepo::new();
        repo.expect_latest()
            .returning(move || Ok(vec![sample(now, MetricKind::Cpu, 20.0)]));
        let audit = MockAudit::stub();
        let collector = Arc::new(std::sync::Mutex::new(
            Box::new(FixedCollector { cpu: 10.0 }) as Box<dyn Collector>,
        ));
        let svc = MonitoringService::new(
            Arc::new(repo),
            collector,
            Arc::new(audit),
            AlertEvaluator::new(&AlertConfig::default()),
            7,
        );
        let latest = svc.latest().await.expect("latest");
        assert_eq!(latest.len(), 1);
        assert_eq!(latest[0].value, 20.0);
    }

    #[tokio::test]
    async fn latest_empty_repo_returns_empty() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_latest().returning(|| Ok(vec![]));
        let audit = MockAudit::stub();
        let collector = Arc::new(std::sync::Mutex::new(
            Box::new(FixedCollector { cpu: 10.0 }) as Box<dyn Collector>,
        ));
        let svc = MonitoringService::new(
            Arc::new(repo),
            collector,
            Arc::new(audit),
            AlertEvaluator::new(&AlertConfig::default()),
            7,
        );
        assert!(svc.latest().await.expect("latest").is_empty());
    }

    #[tokio::test]
    async fn history_delegates_and_preserves_ascending_order() {
        let now = chrono::Utc::now();
        let mut repo = MockSnapshotRepo::new();
        repo.expect_history()
            .withf(|kind, _since| *kind == MetricKind::Cpu)
            .returning(move |_kind, _since| {
                Ok(vec![
                    sample(now, MetricKind::Cpu, 1.0),
                    sample(now + Duration::seconds(30), MetricKind::Cpu, 2.0),
                ])
            });
        let audit = MockAudit::stub();
        let collector = Arc::new(std::sync::Mutex::new(
            Box::new(FixedCollector { cpu: 10.0 }) as Box<dyn Collector>,
        ));
        let svc = MonitoringService::new(
            Arc::new(repo),
            collector,
            Arc::new(audit),
            AlertEvaluator::new(&AlertConfig::default()),
            7,
        );
        let history = svc
            .history(MetricKind::Cpu, now - Duration::hours(1))
            .await
            .expect("history");
        assert_eq!(history[0].value, 1.0);
        assert_eq!(history[1].value, 2.0);
    }

    #[tokio::test]
    async fn alert_firing_records_alert_fired_audit() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_insert().times(4).returning(|_| Ok(()));
        let mut audit = MockAudit::new();
        audit.expect_record().times(1).returning(|event| {
            assert_eq!(event.action, AuditAction::AlertFired);
            assert_eq!(event.target.as_deref(), Some("Cpu"));
            Ok(())
        });
        audit.expect_recent().returning(|_| Ok(vec![]));
        let collector = Arc::new(std::sync::Mutex::new(
            Box::new(FixedCollector { cpu: 95.0 }) as Box<dyn Collector>,
        ));
        let config = AlertConfig {
            cpu_percent: Some(90.0),
            ..AlertConfig::default()
        };
        let svc = MonitoringService::new(
            Arc::new(repo),
            collector,
            Arc::new(audit),
            AlertEvaluator::new(&config),
            7,
        );
        let snap = svc.snapshot_now().await.expect("snapshot");
        let alerts = svc.evaluate_alerts(&snap).await.expect("evaluate");
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, MetricKind::Cpu);
    }
}
