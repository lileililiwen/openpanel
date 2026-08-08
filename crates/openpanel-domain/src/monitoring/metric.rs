//! Metric value objects for the monitoring bounded context.
//!
//! A `MetricSample` is the smallest unit the time series stores: one
//! numeric value of one `MetricKind` at one point in time. Values are
//! guaranteed finite by construction (`MetricSample::new` rejects
//! `NaN` / infinity), and percent-based kinds are clamped into
//! `[0.0, 100.0]` by [`clamp_to_percent`] at collection time.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::monitoring::error::MonitoringError;

/// The resource being measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MetricKind {
    /// Processor utilization as a percentage.
    Cpu,
    /// RAM utilization as a percentage.
    Memory,
    /// Filesystem utilization as a percentage (per mount).
    Disk,
    /// Aggregate network throughput.
    Network,
}

impl MetricKind {
    /// Stable lowercase identifier used in API / CLI output and the
    /// `kind` column.
    pub fn as_str(self) -> &'static str {
        match self {
            MetricKind::Cpu => "Cpu",
            MetricKind::Memory => "Memory",
            MetricKind::Disk => "Disk",
            MetricKind::Network => "Network",
        }
    }
}

impl std::str::FromStr for MetricKind {
    type Err = MonitoringError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Cpu" | "cpu" => Ok(MetricKind::Cpu),
            "Memory" | "memory" => Ok(MetricKind::Memory),
            "Disk" | "disk" => Ok(MetricKind::Disk),
            "Network" | "network" => Ok(MetricKind::Network),
            _ => Err(MonitoringError::InvalidKind(s.to_string())),
        }
    }
}

/// The unit a sample's value is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    /// Percentage, e.g. CPU or memory utilization.
    Percent,
    /// A byte count.
    Bytes,
    /// Bytes per second (throughput).
    BytesPerSecond,
    /// An arbitrary dimensionless gauge (e.g. load average).
    Gauge,
}

/// One measured value of one [`MetricKind`] at one point in time.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MetricSample {
    /// The resource this sample measures.
    pub kind: MetricKind,
    /// Unit the value is expressed in.
    pub unit: Unit,
    /// The measured value. Finite by construction.
    pub value: f64,
    /// When the sample was collected (UTC). Second precision when
    /// persisted; the stored RFC 3339 string is derived from this.
    pub ts: DateTime<Utc>,
}

impl MetricSample {
    /// Create a new sample. Rejects non-finite values (`NaN`,
    /// `+inf`, `-inf`) with `MonitoringError::InvalidValue`.
    pub fn new(
        kind: MetricKind,
        unit: Unit,
        value: f64,
        ts: DateTime<Utc>,
    ) -> Result<Self, MonitoringError> {
        if !value.is_finite() {
            return Err(MonitoringError::InvalidValue(value.to_string()));
        }
        Ok(Self {
            kind,
            unit,
            value,
            ts,
        })
    }
}

/// Clamp any `f64` into the closed interval `[0.0, 100.0]`, mapping
/// `NaN` to `0.0`. Used by collectors so OS-reported percent quirks
/// never escape into the time series.
pub fn clamp_to_percent(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn now() -> chrono::DateTime<chrono::Utc> {
        Utc::now()
    }

    #[test]
    fn accepts_finite_value() {
        let s = MetricSample::new(MetricKind::Cpu, Unit::Percent, 42.5, now()).unwrap();
        assert_eq!(s.kind, MetricKind::Cpu);
        assert_eq!(s.value, 42.5);
    }

    #[test]
    fn rejects_nan() {
        assert!(matches!(
            MetricSample::new(MetricKind::Cpu, Unit::Percent, f64::NAN, now()),
            Err(MonitoringError::InvalidValue(_))
        ));
    }

    #[test]
    fn rejects_infinity() {
        assert!(matches!(
            MetricSample::new(MetricKind::Memory, Unit::Percent, f64::INFINITY, now()),
            Err(MonitoringError::InvalidValue(_))
        ));
        assert!(matches!(
            MetricSample::new(MetricKind::Memory, Unit::Percent, f64::NEG_INFINITY, now()),
            Err(MonitoringError::InvalidValue(_))
        ));
    }

    #[test]
    fn kind_round_trips() {
        for kind in [
            MetricKind::Cpu,
            MetricKind::Memory,
            MetricKind::Disk,
            MetricKind::Network,
        ] {
            assert_eq!(kind.as_str().parse::<MetricKind>(), Ok(kind));
        }
        assert!(matches!(
            "Bogus".parse::<MetricKind>(),
            Err(MonitoringError::InvalidKind(_))
        ));
        assert!(matches!(
            "".parse::<MetricKind>(),
            Err(MonitoringError::InvalidKind(_))
        ));
    }
}

/// Property-based tests for [`clamp_to_percent`].
#[cfg(test)]
mod prop {
    use proptest::prelude::*;

    use super::*;

    proptest! {
        #[test]
        fn prop_clamp_always_within_bounds(v in f64::MIN..f64::MAX) {
            let c = clamp_to_percent(v);
            prop_assert!((0.0..=100.0).contains(&c), "clamp({v}) = {c} out of [0,100]");
        }

        #[test]
        fn prop_clamp_identity_inside_bounds(v in 0.0f64..=100.0) {
            prop_assert_eq!(clamp_to_percent(v), v);
        }

        #[test]
        fn prop_clamp_nan_to_zero(_x in 0u32..10) {
            prop_assert_eq!(clamp_to_percent(f64::NAN), 0.0);
        }
    }
}
