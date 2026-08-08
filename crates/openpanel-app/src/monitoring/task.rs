//! Background collector task: samples the host on a schedule, persists,
//! prunes, and evaluates alerts.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::jobs::BackgroundTask;
use tokio::sync::Notify;

use super::MonitoringService;

/// Scheduler for the monitoring collection loop. Implements
/// [`BackgroundTask`]. The collector lives inside the service; this
/// task only drives the tick schedule.
pub struct MonitoringCollectorTask {
    /// Service used for collection, persistence, pruning, and alert
    /// evaluation.
    pub service: Arc<MonitoringService>,
    /// Seconds between ticks.
    pub interval_secs: u64,
}

impl MonitoringCollectorTask {
    /// Construct a task bound to a service.
    pub fn new(service: Arc<MonitoringService>, interval_secs: u64) -> Self {
        Self {
            service,
            interval_secs,
        }
    }
}

#[async_trait]
impl BackgroundTask for MonitoringCollectorTask {
    fn name(&self) -> &'static str {
        "monitoring-collector"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        // Collect immediately so a fresh DB has data, then tick on the
        // schedule.
        let mut interval =
            tokio::time::interval(std::time::Duration::from_secs(self.interval_secs));
        loop {
            tokio::select! {
                _ = interval.tick() => {}
                _ = shutdown.notified() => break,
            }
            match self.service.tick().await {
                Ok(alerts) => {
                    if !alerts.is_empty() {
                        tracing::warn!(count = alerts.len(), "monitoring: alert fired",);
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "monitoring: collection tick failed");
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use openpanel_core::jobs::BackgroundTask;
    use openpanel_domain::monitoring::{MonitoringError, SystemSnapshot};
    use openpanel_test_support::{MockAudit, MockSnapshotRepo};
    use tokio::sync::Notify;

    use super::super::{
        AlertConfig, AlertEvaluator, Collector, MonitoringCollectorTask, MonitoringService,
    };

    fn fixed_snapshot() -> SystemSnapshot {
        SystemSnapshot::new(
            chrono::Utc::now(),
            0.5,
            95.0,
            40.0,
            vec![openpanel_domain::monitoring::DiskReading {
                mount: "/".into(),
                percent: 50.0,
            }],
            vec![],
        )
        .expect("valid snapshot")
    }

    /// A collector double that returns a fixed snapshot.
    struct OkCollector;

    impl Collector for OkCollector {
        fn snapshot(&mut self) -> Result<SystemSnapshot, MonitoringError> {
            Ok(fixed_snapshot())
        }
    }

    /// A collector double that fails on the first call, then succeeds.
    struct FlakyCollector {
        calls: usize,
    }

    impl Collector for FlakyCollector {
        fn snapshot(&mut self) -> Result<SystemSnapshot, MonitoringError> {
            self.calls += 1;
            if self.calls == 1 {
                Err(MonitoringError::Io("transient disk error".into()))
            } else {
                Ok(fixed_snapshot())
            }
        }
    }

    fn service_with(
        repo: MockSnapshotRepo,
        audit: MockAudit,
        collector: Box<dyn Collector>,
        retention_days: u64,
    ) -> MonitoringService {
        MonitoringService::new(
            Arc::new(repo),
            Arc::new(Mutex::new(collector)),
            Arc::new(audit),
            AlertEvaluator::new(&AlertConfig {
                cpu_percent: Some(90.0),
                ..AlertConfig::default()
            }),
            retention_days,
        )
    }

    #[tokio::test]
    async fn tick_collects_persists_prunes_and_evaluates_alerts() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_insert().times(4).returning(|_| Ok(()));
        repo.expect_prune().times(1).returning(|_| Ok(1));
        let mut audit = MockAudit::new();
        audit.expect_record().times(1).returning(|_| Ok(()));
        audit.expect_recent().returning(|_| Ok(vec![]));

        let svc = service_with(repo, audit, Box::new(OkCollector), 7);
        let alerts = svc.tick().await.expect("tick");
        assert_eq!(alerts.len(), 1, "cpu 95 > threshold 90");
    }

    #[tokio::test]
    async fn failing_collector_does_not_kill_service() {
        let mut repo = MockSnapshotRepo::new();
        // Second tick succeeds -> 4 samples inserted on that tick only.
        repo.expect_insert().times(4).returning(|_| Ok(()));
        repo.expect_prune().times(1).returning(|_| Ok(0));
        let audit = MockAudit::stub();

        let svc = service_with(repo, audit, Box::new(FlakyCollector { calls: 0 }), 7);
        let err = svc.snapshot_now().await;
        assert!(err.is_err(), "first tick must fail");

        let alerts = svc.tick().await.expect("second tick succeeds");
        assert_eq!(alerts.len(), 1);
    }

    #[tokio::test]
    async fn retention_zero_disables_pruning() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_insert().times(4).returning(|_| Ok(()));
        repo.expect_prune().never();
        let audit = MockAudit::stub();

        let svc = service_with(repo, audit, Box::new(OkCollector), 0);
        let _ = svc.tick().await.expect("tick");
    }

    #[tokio::test]
    async fn task_run_tick_loop() {
        let mut repo = MockSnapshotRepo::new();
        repo.expect_insert().returning(|_| Ok(()));
        repo.expect_prune().returning(|_| Ok(0));
        let audit = MockAudit::stub();

        let svc = Arc::new(service_with(repo, audit, Box::new(OkCollector), 7));
        let task = MonitoringCollectorTask::new(svc, 3600);
        let shutdown = Arc::new(Notify::new());

        // Signal shutdown almost immediately; the loop must exit
        // cleanly regardless of whether a tick completed.
        shutdown.notify_one();
        Box::new(task)
            .run(shutdown)
            .await
            .expect("task exits on shutdown");
    }
}
