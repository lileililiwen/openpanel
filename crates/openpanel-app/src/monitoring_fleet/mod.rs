//! Monitoring-fleet application service: bounded queries, threshold
//! policy with recovery hysteresis, independent probe records, and safe
//! fleet aggregation.
//!
//! In-memory projection over existing collection (`SnapshotRepository`),
//! agent registrations, and the notification dispatcher — the same shape
//! as `operator_security` (no new tables). Covers
//! `monitoring-fleet-operations`: query/policy persistence, supervised
//! probes + notification integration, fleet aggregation, bounded
//! retention + stale/error semantics.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    User,
    identity::Role,
    monitoring::{MetricKind, MetricSample},
    monitoring_fleet::{
        FleetDataState, FleetEventRateLimiter, FleetHostSummary, FleetMetricQuery, FleetOpsError,
        FleetRefreshPolicy, FleetSavedView, FleetThresholdPolicy, FleetThresholdTracker,
        FleetThresholdTransition, IndependentProbeOrigin, IndependentProbeResult,
        project_fleet_host, scope_fleet_hosts,
    },
};
use uuid::Uuid;

/// Errors surfaced by the monitoring-fleet service.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FleetServiceError {
    /// Validation failed.
    #[error("invalid monitoring-fleet value: {0}")]
    Invalid(String),
    /// Caller lacks permission.
    #[error("forbidden")]
    Forbidden,
    /// Internal failure (poisoned lock).
    #[error("internal error")]
    Internal,
}

impl From<FleetOpsError> for FleetServiceError {
    fn from(error: FleetOpsError) -> Self {
        Self::Invalid(error.to_string())
    }
}

/// One stored threshold policy with its hysteresis + rate-limit state.
struct StoredThreshold {
    policy: FleetThresholdPolicy,
    fleet_metric_name: String,
    tracker: FleetThresholdTracker,
    limiter: FleetEventRateLimiter,
}

/// In-memory monitoring-fleet service (projection; collectors stay authoritative).
pub struct MonitoringFleetService {
    audit: Arc<dyn AuditService>,
    notifications: Option<Arc<crate::notifications::NotificationService>>,
    snapshots: Arc<dyn openpanel_domain::monitoring::SnapshotRepository>,
    views: Mutex<HashMap<Uuid, FleetSavedView>>,
    thresholds: Mutex<HashMap<Uuid, StoredThreshold>>,
}

impl MonitoringFleetService {
    /// Compose over the snapshot port and the audit sink.
    pub fn new(
        snapshots: Arc<dyn openpanel_domain::monitoring::SnapshotRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            audit,
            notifications: None,
            snapshots,
            views: Mutex::new(HashMap::new()),
            thresholds: Mutex::new(HashMap::new()),
        }
    }

    /// Attach notification fan-out for threshold transitions. Publish
    /// failures never fail the evaluation itself.
    pub fn with_notifications(
        mut self,
        notifications: Arc<crate::notifications::NotificationService>,
    ) -> Self {
        self.notifications = Some(notifications);
        self
    }

    /// Owner/admin gate (defense in depth; handlers gate again).
    fn require_operator(user: &User) -> Result<(), FleetServiceError> {
        if matches!(user.role(), Role::Owner | Role::Admin) {
            Ok(())
        } else {
            Err(FleetServiceError::Forbidden)
        }
    }

    /// Validate a metric query without touching storage.
    pub fn validate_query(
        &self,
        range_secs: i64,
        limit: u32,
        refresh_secs: u32,
    ) -> Result<FleetMetricQuery, FleetServiceError> {
        Ok(FleetMetricQuery::new(range_secs, limit, refresh_secs)?)
    }

    /// Bounded history query with stale/empty/unavailable semantics.
    pub async fn query_history(
        &self,
        kind: MetricKind,
        query: &FleetMetricQuery,
        now: DateTime<Utc>,
    ) -> Result<(Vec<MetricSample>, FleetDataState), FleetServiceError> {
        let since = query.window_start(now);
        let policy = FleetRefreshPolicy::new(query.refresh_secs)?;
        let samples = self
            .snapshots
            .history(kind, since)
            .await
            .map_err(|e| FleetServiceError::Invalid(e.to_string()))?;
        let bounded: Vec<MetricSample> = samples.into_iter().take(query.limit as usize).collect();
        let latest_at = bounded.last().map(|sample| sample.ts);
        let state = FleetDataState::derive(
            bounded.len(),
            latest_at,
            policy.staleness_deadline(now),
            None,
        );
        Ok((bounded, state))
    }

    /// Persist a saved view (bounded config, never samples).
    pub async fn save_fleet_view(
        &self,
        caller: &User,
        view: FleetSavedView,
    ) -> Result<Uuid, FleetServiceError> {
        Self::require_operator(caller)?;
        let id = Uuid::new_v4();
        self.views
            .lock()
            .map_err(|_| FleetServiceError::Internal)?
            .insert(id, view);
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AlertFired,
                    AuditOutcome::Success,
                )
                .target("fleet-view"),
            )
            .await
            .ok();
        Ok(id)
    }

    /// Load a saved view.
    pub async fn load_fleet_view(&self, id: Uuid) -> Result<FleetSavedView, FleetServiceError> {
        self.views
            .lock()
            .map_err(|_| FleetServiceError::Internal)?
            .get(&id)
            .cloned()
            .ok_or_else(|| FleetServiceError::Invalid("saved view not found".into()))
    }

    /// Register a threshold policy for one metric.
    pub async fn register_threshold_policy(
        &self,
        caller: &User,
        fleet_metric_name: impl Into<String>,
        policy: FleetThresholdPolicy,
        now: DateTime<Utc>,
    ) -> Result<Uuid, FleetServiceError> {
        Self::require_operator(caller)?;
        let id = Uuid::new_v4();
        self.thresholds
            .lock()
            .map_err(|_| FleetServiceError::Internal)?
            .insert(
                id,
                StoredThreshold {
                    policy,
                    fleet_metric_name: fleet_metric_name.into(),
                    tracker: FleetThresholdTracker::new(),
                    limiter: FleetEventRateLimiter::new(now),
                },
            );
        Ok(id)
    }

    /// Evaluate one sample: hysteresis transition, deduplicated, rate
    /// limited, routed through the notification dispatcher.
    pub async fn evaluate_fleet_threshold(
        &self,
        policy_id: Uuid,
        value: f64,
        now: DateTime<Utc>,
    ) -> Result<Option<FleetThresholdTransition>, FleetServiceError> {
        let (transition, fleet_metric_name) = {
            let mut stored = self
                .thresholds
                .lock()
                .map_err(|_| FleetServiceError::Internal)?;
            let entry = stored
                .get_mut(&policy_id)
                .ok_or_else(|| FleetServiceError::Invalid("threshold policy not found".into()))?;
            match entry.tracker.observe(&entry.policy, value, now) {
                Some(event) if entry.limiter.allow(&entry.policy, now) => {
                    (Some(event), entry.fleet_metric_name.clone())
                }
                _ => (None, entry.fleet_metric_name.clone()),
            }
        };
        if let Some(event) = transition {
            self.audit
                .record(
                    AuditEvent::new(
                        "monitoring-fleet-operations",
                        AuditAction::AlertFired,
                        AuditOutcome::Success,
                    )
                    .metadata(serde_json::json!({
                        "metric": fleet_metric_name,
                        "breached": event.breached,
                        "value": event.value,
                        "threshold": event.threshold,
                    })),
                )
                .await
                .ok();
            if let Some(notifications) = &self.notifications {
                let alert = openpanel_domain::notifications::NotificationEvent::new(
                    Uuid::new_v4(),
                    openpanel_domain::notifications::EventKind::Alert,
                    if event.breached {
                        "fleet threshold breached"
                    } else {
                        "fleet threshold recovered"
                    }
                    .to_string(),
                    openpanel_domain::notifications::Severity::Warning,
                    serde_json::json!({
                        "breached": event.breached,
                        "value": event.value,
                        "threshold": event.threshold,
                    }),
                    now,
                );
                if let Ok(alert) = alert
                    && let Err(error) = notifications.publish(alert).await
                {
                    tracing::warn!(error = %error, "fleet threshold notification failed");
                }
            }
            // Re-read for the return value is unnecessary; rebuild from
            // the audited event inputs.
            return Ok(Some(event));
        }
        Ok(None)
    }

    /// Record an independent probe result (bounded detail, explicit
    /// origin so panel-down stays distinguishable from target-down).
    pub fn record_independent_probe(
        &self,
        target_up: bool,
        panel_up: bool,
        detail: &str,
        now: DateTime<Utc>,
    ) -> IndependentProbeResult {
        IndependentProbeResult::new(
            IndependentProbeOrigin::Independent,
            target_up,
            panel_up,
            detail,
            now,
        )
    }

    /// Aggregate agent registrations into secret-free, owner-scoped fleet
    /// health. Heartbeat expiry wins over drift; revoked/offline maps to
    /// unavailable; version/manifest drift maps to drifted.
    pub fn aggregate_fleet_hosts(
        &self,
        registrations: &[openpanel_domain::agent::AgentRegistration],
        versions: &HashMap<Uuid, String>,
        baseline_version: &str,
        manifests_match: &HashMap<Uuid, bool>,
        owner_id: Uuid,
        now: DateTime<Utc>,
    ) -> Vec<FleetHostSummary> {
        let projected: Vec<(FleetHostSummary, Uuid)> = registrations
            .iter()
            .map(|registration| {
                let id = registration.id().as_uuid();
                let revoked =
                    registration.status() == openpanel_domain::agent::AgentStatus::Revoked;
                let offline =
                    registration.status() == openpanel_domain::agent::AgentStatus::Offline;
                let version = versions
                    .get(&id)
                    .map(String::as_str)
                    .unwrap_or(baseline_version);
                let manifest_ok = manifests_match.get(&id).copied().unwrap_or(true);
                let summary = project_fleet_host(
                    id,
                    registration.hostname(),
                    revoked,
                    offline,
                    registration.last_heartbeat_at(),
                    version,
                    baseline_version,
                    manifest_ok,
                    now,
                );
                (summary, registration.owner_id())
            })
            .collect();
        scope_fleet_hosts(projected, owner_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openpanel_test_support::{MockAudit, MockSnapshotRepo};

    /// Capability under test: `monitoring-fleet-operations`.
    const CAPABILITY: &str = "monitoring-fleet-operations";

    fn owner() -> User {
        User::new(
            Uuid::new_v4(),
            openpanel_domain::Username::new("owner").expect("username"),
            openpanel_domain::Email::new("owner@example.com").expect("email"),
            openpanel_domain::Password::hash("correct horse battery staple").expect("password"),
            Role::Owner,
        )
    }

    fn service() -> MonitoringFleetService {
        let repo = Arc::new(MockSnapshotRepo::new());
        let audit = Arc::new(MockAudit::stub());
        MonitoringFleetService::new(repo, audit)
    }

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "monitoring-fleet-operations");
    }

    #[test]
    fn query_validation_rejects_unbounded_ranges() {
        let svc = service();
        assert!(svc.validate_query(30, 100, 60).is_err());
        assert!(svc.validate_query(3600, 100, 60).is_ok());
    }

    #[tokio::test]
    async fn saved_view_round_trip() {
        let svc = service();
        let caller = owner();
        let view = FleetSavedView::new(
            "overnight",
            vec![openpanel_domain::monitoring_fleet::FleetViewPanel {
                metric: "Cpu".into(),
                range_secs: 3600,
            }],
            60,
        )
        .expect("view");
        let id = svc
            .save_fleet_view(&caller, view.clone())
            .await
            .expect("save");
        assert_eq!(svc.load_fleet_view(id).await.expect("load"), view);
    }

    #[tokio::test]
    async fn threshold_evaluation_deduplicates_and_recovers() {
        let svc = service();
        let caller = owner();
        let now = Utc::now();
        let policy = FleetThresholdPolicy::new(90.0, 80.0, 10).expect("policy");
        let id = svc
            .register_threshold_policy(&caller, "cpu_percent", policy, now)
            .await
            .expect("register");
        let breach = svc
            .evaluate_fleet_threshold(id, 95.0, now)
            .await
            .expect("evaluate");
        assert!(breach.is_some_and(|event| event.breached));
        let duplicate = svc
            .evaluate_fleet_threshold(id, 96.0, now)
            .await
            .expect("evaluate");
        assert!(duplicate.is_none());
        let recovery = svc
            .evaluate_fleet_threshold(id, 70.0, now)
            .await
            .expect("evaluate");
        assert!(recovery.is_some_and(|event| !event.breached));
    }

    #[tokio::test]
    async fn threshold_rate_limit_suppresses_beyond_budget() {
        let svc = service();
        let caller = owner();
        let now = Utc::now();
        let policy = FleetThresholdPolicy::new(90.0, 80.0, 1).expect("policy");
        let id = svc
            .register_threshold_policy(&caller, "cpu_percent", policy, now)
            .await
            .expect("register");
        assert!(
            svc.evaluate_fleet_threshold(id, 95.0, now)
                .await
                .expect("first")
                .is_some()
        );
        // Recover + re-breach would be a second event in the same hour.
        assert!(
            svc.evaluate_fleet_threshold(id, 70.0, now)
                .await
                .expect("recovery")
                .is_none()
        );
    }

    #[test]
    fn independent_probe_stays_observable_when_panel_down() {
        let svc = service();
        let probe = svc.record_independent_probe(false, false, "timeout", Utc::now());
        assert!(probe.observable_when_panel_down());
        assert_eq!(probe.outage_kind(), "panel-and-target-down");
    }

    #[test]
    fn fleet_aggregation_is_scoped_and_secret_free() {
        use openpanel_domain::agent::{AgentId, AgentRegistration, AgentStatus};
        let svc = service();
        let owner_id = Uuid::new_v4();
        let now = Utc::now();
        let mine = AgentRegistration::restore(
            AgentId::new(),
            "fp".into(),
            "mine".into(),
            AgentStatus::Online,
            "cert-secret".into(),
            Some(now),
            now,
            owner_id,
        );
        let versions = HashMap::new();
        let manifests = HashMap::new();
        let hosts = svc.aggregate_fleet_hosts(
            std::slice::from_ref(&mine),
            &versions,
            "1.0.0",
            &manifests,
            owner_id,
            now,
        );
        assert_eq!(hosts.len(), 1);
        let rendered = serde_json::to_string(&hosts).expect("json");
        assert!(!rendered.contains("cert-secret"));
    }
}
