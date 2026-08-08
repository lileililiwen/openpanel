//! Alert evaluation for the collector task.
//!
//! Thresholds come from the `[monitoring] alert.*` config section.
//! Hysteresis lives here: a rule fires exactly once per crossing, so a
//! flapping value does not spam audit events.

use std::collections::HashMap;

use openpanel_domain::monitoring::{Alert, AlertRule, MetricKind, SystemSnapshot};

/// Optional alert thresholds, one per metric kind. `None` disables the
/// rule for that metric.
#[derive(Debug, Clone, Copy, Default)]
pub struct AlertConfig {
    /// Fire when CPU percent exceeds this value.
    pub cpu_percent: Option<f64>,
    /// Fire when memory percent exceeds this value.
    pub memory_percent: Option<f64>,
    /// Fire when any mounted disk exceeds this percent.
    pub disk_percent: Option<f64>,
}

impl AlertConfig {
    /// Parse the `[monitoring]` config value into thresholds. Missing
    /// or non-numeric entries default to `None` (rule disabled).
    pub fn from_config(value: Option<&serde_json::Value>) -> Self {
        let get = |key: &str| -> Option<f64> {
            value
                .and_then(|v| v.get("alert"))
                .and_then(|a| a.get(key))
                .and_then(|v| v.as_f64())
                .filter(|v| v.is_finite() && *v >= 0.0)
        };
        Self {
            cpu_percent: get("cpu_percent"),
            memory_percent: get("memory_percent"),
            disk_percent: get("disk_percent"),
        }
    }
}

/// Evaluates a snapshot against the configured thresholds, tracking
/// firing state across ticks.
#[derive(Debug, Default)]
pub struct AlertEvaluator {
    rules: HashMap<MetricKind, AlertRule>,
}

impl AlertEvaluator {
    /// Build an evaluator from a parsed [`AlertConfig`]. Only kinds
    /// with a configured threshold get a rule.
    pub fn new(config: &AlertConfig) -> Self {
        let mut rules = HashMap::new();
        if let Some(t) = config.cpu_percent {
            rules.insert(MetricKind::Cpu, AlertRule::new(MetricKind::Cpu, t));
        }
        if let Some(t) = config.memory_percent {
            rules.insert(MetricKind::Memory, AlertRule::new(MetricKind::Memory, t));
        }
        if let Some(t) = config.disk_percent {
            rules.insert(MetricKind::Disk, AlertRule::new(MetricKind::Disk, t));
        }
        Self { rules }
    }

    /// Evaluate every sample in `snapshot` against the rules. Returns
    /// the alerts that fired this tick (usually 0 or 1 per kind).
    pub fn evaluate(&mut self, snapshot: &SystemSnapshot) -> Vec<Alert> {
        let mut fired = Vec::new();
        for sample in snapshot.samples() {
            if let Some(rule) = self.rules.get_mut(&sample.kind)
                && let Some(alert) = rule.evaluate(sample.value)
            {
                fired.push(alert);
            }
        }
        fired
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn snapshot(cpu: f64) -> SystemSnapshot {
        let now = Utc::now();
        SystemSnapshot::new(
            now,
            1.0,
            cpu,
            50.0,
            vec![openpanel_domain::monitoring::DiskReading {
                mount: "/".into(),
                percent: 50.0,
            }],
            vec![],
        )
        .expect("valid snapshot")
    }

    fn evaluate_all(evaluator: &mut AlertEvaluator, values: &[f64]) -> Vec<Alert> {
        values
            .iter()
            .flat_map(|&cpu| evaluator.evaluate(&snapshot(cpu)))
            .collect()
    }

    #[test]
    fn fires_when_value_exceeds_threshold() {
        let config = AlertConfig {
            cpu_percent: Some(90.0),
            ..AlertConfig::default()
        };
        let mut evaluator = AlertEvaluator::new(&config);
        let alerts = evaluate_all(&mut evaluator, &[95.0]);
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, MetricKind::Cpu);
        assert_eq!(alerts[0].value, 95.0);
    }

    #[test]
    fn no_alert_when_below_threshold() {
        let config = AlertConfig {
            cpu_percent: Some(90.0),
            ..AlertConfig::default()
        };
        let mut evaluator = AlertEvaluator::new(&config);
        assert!(evaluate_all(&mut evaluator, &[50.0]).is_empty());
    }

    #[test]
    fn hysteresis_fires_once_until_rearmed() {
        let config = AlertConfig {
            cpu_percent: Some(90.0),
            ..AlertConfig::default()
        };
        let mut evaluator = AlertEvaluator::new(&config);
        let alerts = evaluate_all(&mut evaluator, &[95.0, 95.0, 95.0, 50.0, 95.0]);
        assert_eq!(
            alerts.len(),
            2,
            "first crossing and re-crossing after drop must fire exactly twice"
        );
    }

    #[test]
    fn absent_threshold_never_fires() {
        let evaluator = AlertEvaluator::new(&AlertConfig::default());
        // No rules configured -> no alerts possible regardless of values.
        let mut evaluator = evaluator;
        assert!(evaluate_all(&mut evaluator, &[100.0, 100.0]).is_empty());
    }

    #[test]
    fn parses_thresholds_from_config() {
        let value = serde_json::json!({
            "interval_secs": 60,
            "alert": {
                "cpu_percent": 90.0,
                "disk_percent": 85.5,
            }
        });
        let config = AlertConfig::from_config(Some(&value));
        assert_eq!(config.cpu_percent, Some(90.0));
        assert_eq!(config.memory_percent, None);
        assert_eq!(config.disk_percent, Some(85.5));
    }
}

#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    fn snapshot(cpu: f64) -> SystemSnapshot {
        SystemSnapshot::new(
            chrono::Utc::now(),
            1.0,
            cpu,
            50.0,
            vec![openpanel_domain::monitoring::DiskReading {
                mount: "/".into(),
                percent: 50.0,
            }],
            vec![],
        )
        .expect("valid snapshot")
    }

    proptest! {
        #[test]
        fn prop_alert_count_is_bounded(values in proptest::collection::vec(0.0f64..=100.0, 0..100)) {
            let config = AlertConfig { cpu_percent: Some(50.0), ..AlertConfig::default() };
            let mut evaluator = AlertEvaluator::new(&config);
            // One alert per crossing; never more than one per tick.
            let mut firing = false;
            for v in values {
                let fired = evaluator.evaluate(&snapshot(v));
                if v > 50.0 && !firing {
                    prop_assert_eq!(fired.len(), 1);
                    firing = true;
                } else {
                    prop_assert_eq!(fired.len(), 0);
                }
                if v <= 50.0 {
                    firing = false;
                }
            }
        }
    }
}
