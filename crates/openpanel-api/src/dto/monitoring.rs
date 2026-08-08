//! Request/response DTOs for `/api/v1/monitoring/*`.

use chrono::{DateTime, Utc};
use openpanel_domain::monitoring::{DiskReading, MetricSample, NetworkReading, SystemSnapshot};
use serde::Serialize;

/// One disk utilization entry in an overview response.
#[derive(Debug, Serialize)]
pub struct DiskDto {
    /// Mount point, e.g. `/`.
    pub mount: String,
    /// Utilization as a percentage in `[0.0, 100.0]`.
    pub percent: f64,
}

impl From<&DiskReading> for DiskDto {
    fn from(d: &DiskReading) -> Self {
        Self {
            mount: d.mount.clone(),
            percent: d.percent,
        }
    }
}

/// One network interface throughput entry in an overview response.
#[derive(Debug, Serialize)]
pub struct NetworkDto {
    /// Interface name, e.g. `eth0`.
    pub interface: String,
    /// Inbound bytes per second.
    pub rx_bps: u64,
    /// Outbound bytes per second.
    pub tx_bps: u64,
}

impl From<&NetworkReading> for NetworkDto {
    fn from(n: &NetworkReading) -> Self {
        Self {
            interface: n.interface.clone(),
            rx_bps: n.rx_bytes_per_sec,
            tx_bps: n.tx_bytes_per_sec,
        }
    }
}

/// The current snapshot (`GET /api/v1/monitoring/overview`).
#[derive(Debug, Serialize)]
pub struct OverviewDto {
    /// When the snapshot was collected.
    pub timestamp: DateTime<Utc>,
    /// 1-minute load average.
    pub load: f64,
    /// CPU utilization as a percentage.
    pub cpu: f64,
    /// Memory utilization as a percentage.
    pub memory: f64,
    /// One entry per mounted filesystem.
    pub disk: Vec<DiskDto>,
    /// One entry per network interface.
    pub network: Vec<NetworkDto>,
}

impl From<&SystemSnapshot> for OverviewDto {
    fn from(s: &SystemSnapshot) -> Self {
        Self {
            timestamp: s.timestamp,
            load: s.load,
            cpu: s.cpu,
            memory: s.memory,
            disk: s.disk.iter().map(DiskDto::from).collect(),
            network: s.network.iter().map(NetworkDto::from).collect(),
        }
    }
}

/// One point of a history series (`GET /api/v1/monitoring/history`).
#[derive(Debug, Serialize)]
pub struct HistoryPointDto {
    /// Sample timestamp.
    pub timestamp: DateTime<Utc>,
    /// Measured value.
    pub value: f64,
}

impl From<&MetricSample> for HistoryPointDto {
    fn from(s: &MetricSample) -> Self {
        Self {
            timestamp: s.ts,
            value: s.value,
        }
    }
}
