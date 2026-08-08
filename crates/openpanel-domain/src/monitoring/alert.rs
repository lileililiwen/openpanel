//! Alert value objects for the monitoring bounded context.
//!
//! An `AlertRule` maps a `MetricKind` + threshold to an alert that
//! fires when a measured value crosses the threshold. The `firing`
//! flag provides hysteresis: an alert fires exactly once per
//! crossing, not on every tick while the value stays above the
//! threshold.

use crate::monitoring::metric::MetricKind;

/// A single alert event produced when a rule fires.
#[derive(Debug, Clone, PartialEq)]
pub struct Alert {
    /// The metric that exceeded its threshold.
    pub kind: MetricKind,
    /// The measured value that triggered the alert.
    pub value: f64,
    /// The threshold that was crossed.
    pub threshold: f64,
}

/// Threshold + hysteresis state for one metric.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertRule {
    /// The metric this rule watches.
    pub kind: MetricKind,
    /// Value above which the alert fires.
    pub threshold: f64,
    /// Whether the alert is currently "firing" (waiting for the value
    /// to drop back below `threshold` before re-arming).
    pub firing: bool,
}

impl AlertRule {
    /// Create a new rule for a metric with the given threshold.
    pub fn new(kind: MetricKind, threshold: f64) -> Self {
        Self {
            kind,
            threshold,
            firing: false,
        }
    }

    /// Evaluate a measured value against this rule. Returns `Some`
    /// (and flips to `firing`) exactly when `value > threshold` and
    /// the rule was not already firing. When `value <= threshold`,
    /// the rule re-arms and returns `None`.
    pub fn evaluate(&mut self, value: f64) -> Option<Alert> {
        if value > self.threshold {
            if self.firing {
                return None;
            }
            self.firing = true;
            Some(Alert {
                kind: self.kind,
                value,
                threshold: self.threshold,
            })
        } else {
            self.firing = false;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_when_value_exceeds_threshold() {
        let mut rule = AlertRule::new(MetricKind::Cpu, 90.0);
        let alert = rule.evaluate(95.0);
        assert!(
            matches!(alert, Some(a) if a.kind == MetricKind::Cpu && a.value == 95.0 && a.threshold == 90.0)
        );
        assert!(rule.firing);
    }

    #[test]
    fn no_alert_when_below_threshold() {
        let mut rule = AlertRule::new(MetricKind::Cpu, 90.0);
        assert!(rule.evaluate(50.0).is_none());
        assert!(!rule.firing);
    }

    #[test]
    fn no_alert_when_equal_to_threshold() {
        let mut rule = AlertRule::new(MetricKind::Cpu, 90.0);
        assert!(rule.evaluate(90.0).is_none());
    }

    #[test]
    fn hysteresis_fires_once_until_rearmed() {
        let mut rule = AlertRule::new(MetricKind::Cpu, 90.0);
        assert!(rule.evaluate(95.0).is_some(), "first crossing fires");
        assert!(rule.evaluate(96.0).is_none(), "still firing -> no re-fire");
        assert!(rule.evaluate(97.0).is_none());
        rule.evaluate(50.0);
        assert!(!rule.firing, "below threshold re-arms");
        assert!(rule.evaluate(95.0).is_some(), "re-crossing fires again");
    }
}
