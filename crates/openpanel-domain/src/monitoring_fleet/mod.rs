//! Monitoring + fleet operations: configurable views, threshold policy
//! with recovery hysteresis, independent uptime probes, and safe fleet
//! aggregation.
//!
//! I/O-free projection over existing collection (`MonitoringService`,
//! `SnapshotRepository`), synthetic probes (`SyntheticCheck`), agent
//! heartbeats (`AgentRegistration`), and the notification dispatcher.
//! Covers `monitoring-fleet-operations`: Configurable Views, Threshold
//! Hysteresis, Independent Uptime, Safe Scoped Fleet Health. Collection,
//! persistence, probing, and delivery stay in the app layer; this module
//! owns validation, state transitions, and secret-free projections.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Domain validation failures.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FleetOpsError {
    /// A value is invalid.
    #[error("invalid monitoring-fleet value: {0}")]
    Invalid(String),
}

/// Bounded metric-query: which metric, how far back, how many points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetMetricQuery {
    /// Look-back window in seconds.
    pub range_secs: i64,
    /// Maximum points to return.
    pub limit: u32,
    /// Refresh interval in seconds.
    pub refresh_secs: u32,
}

impl FleetMetricQuery {
    /// Smallest supported range (60s).
    pub const MIN_RANGE_SECS: i64 = 60;
    /// Largest supported range (30 days).
    pub const MAX_RANGE_SECS: i64 = 30 * 24 * 3600;
    /// Smallest page.
    pub const MIN_LIMIT: u32 = 1;
    /// Largest page (bounded retention).
    pub const MAX_LIMIT: u32 = 5_000;
    /// Fastest refresh.
    pub const MIN_REFRESH_SECS: u32 = 15;
    /// Slowest refresh.
    pub const MAX_REFRESH_SECS: u32 = 600;

    /// Validate bounds; rejects unbounded queries.
    pub fn new(range_secs: i64, limit: u32, refresh_secs: u32) -> Result<Self, FleetOpsError> {
        if !(Self::MIN_RANGE_SECS..=Self::MAX_RANGE_SECS).contains(&range_secs) {
            return Err(FleetOpsError::Invalid(format!(
                "range_secs must be {}..={}",
                Self::MIN_RANGE_SECS,
                Self::MAX_RANGE_SECS
            )));
        }
        if !(Self::MIN_LIMIT..=Self::MAX_LIMIT).contains(&limit) {
            return Err(FleetOpsError::Invalid(format!(
                "limit must be {}..={}",
                Self::MIN_LIMIT,
                Self::MAX_LIMIT
            )));
        }
        if !(Self::MIN_REFRESH_SECS..=Self::MAX_REFRESH_SECS).contains(&refresh_secs) {
            return Err(FleetOpsError::Invalid(format!(
                "refresh_secs must be {}..={}",
                Self::MIN_REFRESH_SECS,
                Self::MAX_REFRESH_SECS
            )));
        }
        Ok(Self {
            range_secs,
            limit,
            refresh_secs,
        })
    }

    /// Start of the window for `now`.
    pub fn window_start(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - chrono::Duration::seconds(self.range_secs)
    }
}

/// Refresh policy: bounded interval with staleness derivation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetRefreshPolicy {
    /// Interval in seconds.
    pub interval_secs: u32,
}

impl FleetRefreshPolicy {
    /// Build a validated policy.
    pub fn new(interval_secs: u32) -> Result<Self, FleetOpsError> {
        if !(FleetMetricQuery::MIN_REFRESH_SECS..=FleetMetricQuery::MAX_REFRESH_SECS)
            .contains(&interval_secs)
        {
            return Err(FleetOpsError::Invalid(format!(
                "refresh interval must be {}..={}",
                FleetMetricQuery::MIN_REFRESH_SECS,
                FleetMetricQuery::MAX_REFRESH_SECS
            )));
        }
        Ok(Self { interval_secs })
    }

    /// Data older than twice the interval is stale.
    pub fn staleness_deadline(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - chrono::Duration::seconds(i64::from(self.interval_secs) * 2)
    }
}

/// Saved dashboard view: bounded configuration, never metric data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetSavedView {
    name: String,
    panels: Vec<FleetViewPanel>,
    refresh_secs: u32,
}

/// One panel inside a saved view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetViewPanel {
    /// Metric identifier (`Cpu`, `Memory`, `Disk`, `Network`).
    pub metric: String,
    /// Range in seconds.
    pub range_secs: i64,
}

impl FleetSavedView {
    /// Maximum panels per view.
    pub const MAX_PANELS: usize = 12;

    /// Validate name/panels/refresh without storing samples.
    pub fn new(
        name: impl Into<String>,
        panels: Vec<FleetViewPanel>,
        refresh_secs: u32,
    ) -> Result<Self, FleetOpsError> {
        let name = name.into();
        if name.trim().is_empty() || name.len() > 64 {
            return Err(FleetOpsError::Invalid("view name is required".into()));
        }
        if panels.is_empty() || panels.len() > Self::MAX_PANELS {
            return Err(FleetOpsError::Invalid(format!(
                "panels must be 1..={}",
                Self::MAX_PANELS
            )));
        }
        FleetRefreshPolicy::new(refresh_secs)?;
        for panel in &panels {
            FleetMetricQuery::new(panel.range_secs, 100, refresh_secs)?;
            if ![
                "Cpu", "Memory", "Disk", "Network", "cpu", "memory", "disk", "network",
            ]
            .contains(&panel.metric.as_str())
            {
                return Err(FleetOpsError::Invalid(format!(
                    "unknown metric `{}`",
                    panel.metric
                )));
            }
        }
        Ok(Self {
            name,
            panels,
            refresh_secs,
        })
    }

    /// View name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Panels.
    pub fn panels(&self) -> &[FleetViewPanel] {
        &self.panels
    }

    /// Refresh interval.
    pub fn refresh_secs(&self) -> u32 {
        self.refresh_secs
    }
}

/// Query result semantics shown in charts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetDataState {
    /// Fresh data is available.
    Ready,
    /// No samples exist in the range.
    Empty,
    /// Newest sample is older than the staleness deadline.
    Stale {
        /// Last observation, if any.
        last_seen_at: Option<DateTime<Utc>>,
    },
    /// Collection failed; charts show the safe message.
    Unavailable {
        /// Safe operator message (never secrets).
        reason: String,
    },
}

impl FleetDataState {
    /// Derive state from sample count + freshness.
    pub fn derive(
        sample_count: usize,
        latest_at: Option<DateTime<Utc>>,
        deadline: DateTime<Utc>,
        collection_error: Option<&str>,
    ) -> Self {
        if let Some(reason) = collection_error {
            return Self::Unavailable {
                reason: reason.to_string(),
            };
        }
        if sample_count == 0 {
            return Self::Empty;
        }
        match latest_at {
            Some(ts) if ts < deadline => Self::Stale {
                last_seen_at: Some(ts),
            },
            _ => Self::Ready,
        }
    }
}

/// Threshold policy with separate breach + recovery levels (hysteresis).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FleetThresholdPolicy {
    /// Value that opens a breach.
    pub breach_at: f64,
    /// Value that closes it (must be below `breach_at`).
    pub recovery_at: f64,
    /// Maximum transitions per hour (deduplication + rate limit).
    pub max_events_per_hour: u32,
}

impl FleetThresholdPolicy {
    /// Build a policy; recovery must sit strictly below breach.
    pub fn new(
        breach_at: f64,
        recovery_at: f64,
        max_events_per_hour: u32,
    ) -> Result<Self, FleetOpsError> {
        if !breach_at.is_finite() || !recovery_at.is_finite() {
            return Err(FleetOpsError::Invalid("thresholds must be finite".into()));
        }
        if recovery_at >= breach_at {
            return Err(FleetOpsError::Invalid(
                "recovery_at must be below breach_at".into(),
            ));
        }
        if max_events_per_hour == 0 || max_events_per_hour > 60 {
            return Err(FleetOpsError::Invalid(
                "max_events_per_hour must be 1..=60".into(),
            ));
        }
        Ok(Self {
            breach_at,
            recovery_at,
            max_events_per_hour,
        })
    }
}

/// Breach lifecycle owned by the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetThresholdState {
    /// Value is below breach.
    Ok,
    /// Value crossed breach; waiting for recovery.
    Breached,
}

/// One transition event (breach or recovery); emitted once per change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FleetThresholdTransition {
    /// Whether the breach opened (`true`) or recovered (`false`).
    pub breached: bool,
    /// Observed value.
    pub value: f64,
    /// Threshold that decided the transition.
    pub threshold: f64,
    /// When observed.
    pub observed_at: DateTime<Utc>,
}

/// Stateful hysteresis tracker: one event per state change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FleetThresholdTracker {
    state: FleetThresholdState,
}

impl FleetThresholdTracker {
    /// Start in `Ok`.
    pub fn new() -> Self {
        Self {
            state: FleetThresholdState::Ok,
        }
    }

    /// Current state.
    pub fn state(&self) -> FleetThresholdState {
        self.state
    }

    /// Evaluate one sample; returns a transition only on state change.
    pub fn observe(
        &mut self,
        policy: &FleetThresholdPolicy,
        value: f64,
        now: DateTime<Utc>,
    ) -> Option<FleetThresholdTransition> {
        match self.state {
            FleetThresholdState::Ok if value > policy.breach_at => {
                self.state = FleetThresholdState::Breached;
                Some(FleetThresholdTransition {
                    breached: true,
                    value,
                    threshold: policy.breach_at,
                    observed_at: now,
                })
            }
            FleetThresholdState::Breached if value <= policy.recovery_at => {
                self.state = FleetThresholdState::Ok;
                Some(FleetThresholdTransition {
                    breached: false,
                    value,
                    threshold: policy.recovery_at,
                    observed_at: now,
                })
            }
            _ => None,
        }
    }
}

impl Default for FleetThresholdTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Hourly rate limiter for threshold transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetEventRateLimiter {
    window_start: DateTime<Utc>,
    count: u32,
}

impl FleetEventRateLimiter {
    /// Start a window at `now`.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self {
            window_start: now,
            count: 0,
        }
    }

    /// Whether one more event fits in the policy budget.
    pub fn allow(&mut self, policy: &FleetThresholdPolicy, now: DateTime<Utc>) -> bool {
        if now - self.window_start >= chrono::Duration::hours(1) {
            self.window_start = now;
            self.count = 0;
        }
        if self.count >= policy.max_events_per_hour {
            return false;
        }
        self.count += 1;
        true
    }
}

/// Where a probe executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IndependentProbeOrigin {
    /// Ran inside the panel process.
    PanelLocal,
    /// Ran outside the panel (supervised task / agent path).
    Independent,
}

/// Bounded probe result with an explicit execution boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndependentProbeResult {
    /// Where the probe ran.
    pub origin: IndependentProbeOrigin,
    /// Whether the target answered.
    pub target_up: bool,
    /// Whether the panel process was reachable when the probe ran.
    pub panel_up: bool,
    /// Bounded detail (never response bodies).
    pub detail: String,
    /// When the probe ran.
    pub ran_at: DateTime<Utc>,
}

impl IndependentProbeResult {
    /// Build a result; detail is truncated to 280 chars.
    pub fn new(
        origin: IndependentProbeOrigin,
        target_up: bool,
        panel_up: bool,
        detail: impl Into<String>,
        ran_at: DateTime<Utc>,
    ) -> Self {
        let mut detail = detail.into();
        if detail.len() > 280 {
            detail.truncate(280);
        }
        Self {
            origin,
            target_up,
            panel_up,
            detail,
            ran_at,
        }
    }

    /// Target-down while the panel is down stays observable through the
    /// independent path (the panel process is not required to read it).
    pub fn observable_when_panel_down(&self) -> bool {
        self.origin == IndependentProbeOrigin::Independent
    }

    /// Distinguish panel-down from target-down.
    pub fn outage_kind(&self) -> &'static str {
        match (self.panel_up, self.target_up) {
            (true, true) => "healthy",
            (true, false) => "target-down",
            (false, true) => "panel-down",
            (false, false) => "panel-and-target-down",
        }
    }
}

/// Fleet host health owned by the aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FleetHealthState {
    /// Heartbeat fresh, version current.
    Healthy,
    /// Heartbeat missed; last-seen + recovery guidance shown.
    Stale,
    /// Version or manifest drifted from the baseline.
    Drifted,
    /// Revoked or explicitly offline.
    Unavailable,
}

impl FleetHealthState {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Stale => "stale",
            Self::Drifted => "drifted",
            Self::Unavailable => "unavailable",
        }
    }
}

/// Secret-free fleet host projection (no certs, tokens, or keys).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetHostSummary {
    /// Host id.
    pub id: Uuid,
    /// Reported hostname.
    pub hostname: String,
    /// Health.
    pub health: FleetHealthState,
    /// Last heartbeat, if any.
    pub last_seen_at: Option<DateTime<Utc>>,
    /// Recovery guidance shown for stale hosts.
    pub recovery_guidance: String,
}

impl FleetHostSummary {
    /// Recovery copy for stale hosts.
    pub fn stale_guidance() -> String {
        "Agent missed its heartbeat. Check the agent service on the host, then re-register if the host was rebuilt.".to_string()
    }
}

/// Heartbeat deadline in seconds (missed heartbeats mark hosts stale).
pub const FLEET_HEARTBEAT_DEADLINE_SECS: i64 = 300;

/// Aggregate one host: heartbeat expiry first, then revocation/offline,
/// then version/manifest drift, else healthy.
#[allow(clippy::too_many_arguments)]
pub fn project_fleet_host(
    id: Uuid,
    hostname: &str,
    revoked: bool,
    offline: bool,
    last_seen_at: Option<DateTime<Utc>>,
    version: &str,
    baseline_version: &str,
    manifest_matches: bool,
    now: DateTime<Utc>,
) -> FleetHostSummary {
    let stale = match last_seen_at {
        None => true,
        Some(ts) => now - ts > chrono::Duration::seconds(FLEET_HEARTBEAT_DEADLINE_SECS),
    };
    if stale {
        return FleetHostSummary {
            id,
            hostname: hostname.to_string(),
            health: FleetHealthState::Stale,
            last_seen_at,
            recovery_guidance: FleetHostSummary::stale_guidance(),
        };
    }
    if revoked || offline {
        return FleetHostSummary {
            id,
            hostname: hostname.to_string(),
            health: FleetHealthState::Unavailable,
            last_seen_at,
            recovery_guidance: String::new(),
        };
    }
    if version != baseline_version || !manifest_matches {
        return FleetHostSummary {
            id,
            hostname: hostname.to_string(),
            health: FleetHealthState::Drifted,
            last_seen_at,
            recovery_guidance: "Version or manifest drifted from the baseline. Roll the agent forward to the pinned release.".to_string(),
        };
    }
    FleetHostSummary {
        id,
        hostname: hostname.to_string(),
        health: FleetHealthState::Healthy,
        last_seen_at,
        recovery_guidance: String::new(),
    }
}

/// Scope a fleet projection to one owner's hosts.
pub fn scope_fleet_hosts(
    hosts: Vec<(FleetHostSummary, Uuid)>,
    owner_id: Uuid,
) -> Vec<FleetHostSummary> {
    hosts
        .into_iter()
        .filter_map(|(summary, owner)| (owner == owner_id).then_some(summary))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Capability under test: `monitoring-fleet-operations`.
    const CAPABILITY: &str = "monitoring-fleet-operations";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 13, 0, 0, 0)
            .single()
            .expect("test time")
    }

    #[test]
    fn capability_marker_matches_spec() {
        assert_eq!(CAPABILITY, "monitoring-fleet-operations");
    }

    #[test]
    fn metric_query_rejects_unbounded_ranges() {
        assert!(FleetMetricQuery::new(30, 100, 60).is_err());
        assert!(FleetMetricQuery::new(3600, 0, 60).is_err());
        assert!(FleetMetricQuery::new(3600, 100, 5).is_err());
        assert!(FleetMetricQuery::new(3600, 100, 60).is_ok());
    }

    #[test]
    fn metric_query_window_start_matches_range() {
        let query = FleetMetricQuery::new(3600, 100, 60).expect("query");
        assert_eq!(
            query.window_start(now()),
            now() - chrono::Duration::seconds(3600)
        );
    }

    #[test]
    fn saved_view_validation_rejects_bad_panels() {
        let panel = FleetViewPanel {
            metric: "Cpu".into(),
            range_secs: 3600,
        };
        assert!(FleetSavedView::new("", vec![panel.clone()], 60).is_err());
        assert!(FleetSavedView::new("view", vec![], 60).is_err());
        assert!(
            FleetSavedView::new(
                "view",
                vec![FleetViewPanel {
                    metric: "Bogus".into(),
                    range_secs: 3600
                }],
                60
            )
            .is_err()
        );
        assert!(FleetSavedView::new("view", vec![panel], 60).is_ok());
    }

    #[test]
    fn data_state_covers_empty_stale_and_unavailable() {
        let deadline = now();
        assert_eq!(
            FleetDataState::derive(0, None, deadline, None),
            FleetDataState::Empty
        );
        assert_eq!(
            FleetDataState::derive(3, None, deadline, None),
            FleetDataState::Ready
        );
        assert_eq!(
            FleetDataState::derive(2, Some(now() - chrono::Duration::hours(2)), now(), None),
            FleetDataState::Stale {
                last_seen_at: Some(now() - chrono::Duration::hours(2))
            }
        );
        assert!(matches!(
            FleetDataState::derive(2, None, deadline, Some("collector failed")),
            FleetDataState::Unavailable { .. }
        ));
    }

    #[test]
    fn threshold_hysteresis_emits_one_transition_per_change() {
        let policy = FleetThresholdPolicy::new(90.0, 80.0, 10).expect("policy");
        let mut tracker = FleetThresholdTracker::new();
        assert!(tracker.observe(&policy, 50.0, now()).is_none());
        let breach = tracker.observe(&policy, 95.0, now()).expect("breach");
        assert!(breach.breached);
        assert!(tracker.observe(&policy, 96.0, now()).is_none());
        assert!(tracker.observe(&policy, 85.0, now()).is_none());
        let recovery = tracker.observe(&policy, 75.0, now()).expect("recovery");
        assert!(!recovery.breached);
        assert_eq!(tracker.state(), FleetThresholdState::Ok);
    }

    #[test]
    fn threshold_policy_rejects_recovery_above_breach() {
        assert!(FleetThresholdPolicy::new(80.0, 90.0, 10).is_err());
        assert!(FleetThresholdPolicy::new(f64::NAN, 80.0, 10).is_err());
        assert!(FleetThresholdPolicy::new(90.0, 80.0, 0).is_err());
    }

    #[test]
    fn rate_limiter_bounds_events_per_hour() {
        let policy = FleetThresholdPolicy::new(90.0, 80.0, 2).expect("policy");
        let mut limiter = FleetEventRateLimiter::new(now());
        assert!(limiter.allow(&policy, now()));
        assert!(limiter.allow(&policy, now()));
        assert!(!limiter.allow(&policy, now()));
        assert!(limiter.allow(&policy, now() + chrono::Duration::hours(2)));
    }

    #[test]
    fn independent_probe_distinguishes_panel_down_from_target_down() {
        let probe = IndependentProbeResult::new(
            IndependentProbeOrigin::Independent,
            false,
            false,
            "target timeout",
            now(),
        );
        assert!(probe.observable_when_panel_down());
        assert_eq!(probe.outage_kind(), "panel-and-target-down");
        let local = IndependentProbeResult::new(
            IndependentProbeOrigin::PanelLocal,
            false,
            true,
            "conn refused",
            now(),
        );
        assert!(!local.observable_when_panel_down());
        assert_eq!(local.outage_kind(), "target-down");
    }

    #[test]
    fn fleet_projection_marks_expired_heartbeat_stale() {
        let summary = project_fleet_host(
            Uuid::new_v4(),
            "web-01",
            false,
            false,
            Some(now() - chrono::Duration::seconds(600)),
            "1.0.0",
            "1.0.0",
            true,
            now(),
        );
        assert_eq!(summary.health, FleetHealthState::Stale);
        assert!(summary.recovery_guidance.contains("heartbeat"));
    }

    #[test]
    fn fleet_projection_flags_version_and_manifest_drift() {
        let drifted = project_fleet_host(
            Uuid::new_v4(),
            "web-01",
            false,
            false,
            Some(now()),
            "0.9.0",
            "1.0.0",
            true,
            now(),
        );
        assert_eq!(drifted.health, FleetHealthState::Drifted);
        let mismatch = project_fleet_host(
            Uuid::new_v4(),
            "web-01",
            false,
            false,
            Some(now()),
            "1.0.0",
            "1.0.0",
            false,
            now(),
        );
        assert_eq!(mismatch.health, FleetHealthState::Drifted);
    }

    #[test]
    fn fleet_scope_filters_to_owner_without_secrets() {
        let owner = Uuid::new_v4();
        let other = Uuid::new_v4();
        let mine = project_fleet_host(
            Uuid::new_v4(),
            "mine",
            false,
            false,
            Some(now()),
            "1.0.0",
            "1.0.0",
            true,
            now(),
        );
        let theirs = project_fleet_host(
            Uuid::new_v4(),
            "theirs",
            false,
            false,
            Some(now()),
            "1.0.0",
            "1.0.0",
            true,
            now(),
        );
        let scoped = scope_fleet_hosts(vec![(mine.clone(), owner), (theirs, other)], owner);
        assert_eq!(scoped, vec![mine]);
        let rendered = serde_json::to_string(&scoped).expect("json");
        assert!(!rendered.contains("cert"));
        assert!(!rendered.contains("token"));
    }

    mod prop {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_query_validation_is_bounded(
                range in -1_000_000i64..10_000_000i64,
                limit in 0u32..10_000u32,
                refresh in 0u32..3_600u32,
            ) {
                let result = FleetMetricQuery::new(range, limit, refresh);
                if (FleetMetricQuery::MIN_RANGE_SECS..=FleetMetricQuery::MAX_RANGE_SECS).contains(&range)
                    && (FleetMetricQuery::MIN_LIMIT..=FleetMetricQuery::MAX_LIMIT).contains(&limit)
                    && (FleetMetricQuery::MIN_REFRESH_SECS..=FleetMetricQuery::MAX_REFRESH_SECS).contains(&refresh)
                {
                    prop_assert!(result.is_ok());
                } else {
                    prop_assert!(result.is_err());
                }
            }

            #[test]
            fn prop_threshold_never_flaps_without_recovery(
                values in proptest::collection::vec(0.0f64..120.0, 1..32),
            ) {
                let policy = FleetThresholdPolicy::new(90.0, 80.0, 60).expect("policy");
                let at = Utc::now();
                let mut tracker = FleetThresholdTracker::new();
                let mut breaches = 0u32;
                for value in values {
                    if tracker.observe(&policy, value, at).is_some_and(|t| t.breached) {
                        breaches += 1;
                        prop_assert_eq!(tracker.state(), FleetThresholdState::Breached);
                    }
                }
                prop_assert!(breaches <= 16);
            }
        }
    }
}
