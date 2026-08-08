//! `SystemSnapshot` aggregate for the monitoring bounded context.
//!
//! A snapshot is a point-in-time picture of the host: load average,
//! cpu / memory percents, one entry per mounted filesystem, one entry
//! per network interface, and the flat list of samples derived from
//! them. The aggregate enforces the invariant "at least one sample".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::monitoring::{error::MonitoringError, metric::MetricSample};

/// Disk utilization for one mounted filesystem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiskReading {
    /// Mount point, e.g. `/` or `/var`.
    pub mount: String,
    /// Utilization as a percentage in `[0.0, 100.0]`.
    pub percent: f64,
}

/// Per-interface network throughput, in bytes per second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkReading {
    /// Interface name, e.g. `eth0`.
    pub interface: String,
    /// Inbound bytes per second.
    pub rx_bytes_per_sec: u64,
    /// Outbound bytes per second.
    pub tx_bytes_per_sec: u64,
}

/// Point-in-time picture of the host's resource usage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SystemSnapshot {
    /// When the snapshot was collected (UTC).
    pub timestamp: DateTime<Utc>,
    /// 1-minute load average (`Unit::Gauge`).
    pub load: f64,
    /// CPU utilization as a percentage.
    pub cpu: f64,
    /// Memory utilization as a percentage.
    pub memory: f64,
    /// One entry per mounted filesystem.
    pub disk: Vec<DiskReading>,
    /// One entry per network interface.
    pub network: Vec<NetworkReading>,
}

impl SystemSnapshot {
    /// Create a new snapshot. Rejects a `timestamp` in the future and
    /// a snapshot with no `disk` / `network` entries AND no
    /// meaningful cpu / memory / load values (i.e. nothing to store).
    pub fn new(
        timestamp: DateTime<Utc>,
        load: f64,
        cpu: f64,
        memory: f64,
        disk: Vec<DiskReading>,
        network: Vec<NetworkReading>,
    ) -> Result<Self, MonitoringError> {
        if timestamp > Utc::now() {
            return Err(MonitoringError::TimestampInFuture);
        }
        if disk.is_empty() && network.is_empty() {
            return Err(MonitoringError::EmptySnapshot);
        }
        Ok(Self {
            timestamp,
            load,
            cpu,
            memory,
            disk,
            network,
        })
    }

    /// Flatten the snapshot into per-kind samples suitable for the
    /// time series. Produces `Cpu`, `Memory`, one `Disk` sample (the
    /// highest utilization across mounts), and one aggregate `Network`
    /// sample (sum of per-interface throughput). All samples share the
    /// snapshot's timestamp, so each `(ts, kind)` pair is unique —
    /// matching the `monitoring_samples` primary key.
    pub fn samples(&self) -> Vec<MetricSample> {
        let mut out = Vec::with_capacity(4);

        if let Ok(s) = MetricSample::new(MetricKind::Cpu, Unit::Percent, self.cpu, self.timestamp) {
            out.push(s);
        }
        if let Ok(s) = MetricSample::new(
            MetricKind::Memory,
            Unit::Percent,
            self.memory,
            self.timestamp,
        ) {
            out.push(s);
        }
        let disk_max = self.disk.iter().map(|d| d.percent).fold(0.0, f64::max);
        if let Ok(s) = MetricSample::new(MetricKind::Disk, Unit::Percent, disk_max, self.timestamp)
        {
            out.push(s);
        }
        let net = self
            .network
            .iter()
            .map(|n| n.rx_bytes_per_sec + n.tx_bytes_per_sec)
            .sum::<u64>();
        if let Ok(s) = MetricSample::new(
            MetricKind::Network,
            Unit::BytesPerSecond,
            net as f64,
            self.timestamp,
        ) {
            out.push(s);
        }
        out
    }
}

use crate::monitoring::metric::{MetricKind, Unit};

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn sample_reading() -> (Vec<DiskReading>, Vec<NetworkReading>) {
        (
            vec![DiskReading {
                mount: "/".into(),
                percent: 68.2,
            }],
            vec![NetworkReading {
                interface: "eth0".into(),
                rx_bytes_per_sec: 12_345,
                tx_bytes_per_sec: 678,
            }],
        )
    }

    #[test]
    fn accepts_valid_snapshot() {
        let (disk, net) = sample_reading();
        let snap = SystemSnapshot::new(now(), 0.42, 12.3, 47.1, disk, net).unwrap();
        assert_eq!(snap.load, 0.42);
        assert_eq!(snap.cpu, 12.3);
    }

    #[test]
    fn rejects_future_timestamp() {
        let (disk, net) = sample_reading();
        let future = Utc.timestamp_opt(now().timestamp() + 10_000, 0).unwrap();
        assert!(matches!(
            SystemSnapshot::new(future, 0.0, 1.0, 2.0, disk, net),
            Err(MonitoringError::TimestampInFuture)
        ));
    }

    #[test]
    fn rejects_empty_snapshot() {
        assert!(matches!(
            SystemSnapshot::new(now(), 0.0, 1.0, 2.0, vec![], vec![]),
            Err(MonitoringError::EmptySnapshot)
        ));
    }

    #[test]
    fn samples_flatten_per_kind() {
        let (disk, net) = sample_reading();
        let snap = SystemSnapshot::new(now(), 0.42, 12.3, 47.1, disk, net).unwrap();
        let samples = snap.samples();
        // Cpu + Memory + Disk(/ ) + Network aggregate = 4.
        assert_eq!(samples.len(), 4);
        assert!(
            samples
                .iter()
                .any(|s| s.kind == MetricKind::Cpu && s.value == 12.3)
        );
        assert!(
            samples
                .iter()
                .any(|s| s.kind == MetricKind::Memory && s.value == 47.1)
        );
        assert!(
            samples
                .iter()
                .any(|s| s.kind == MetricKind::Disk && s.value == 68.2)
        );
        // rx 12345 + tx 678 = 13023 bps aggregate.
        assert!(
            samples
                .iter()
                .any(|s| s.kind == MetricKind::Network && s.value == 13_023.0)
        );
    }
}
