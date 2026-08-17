//! Monitoring bounded context: metric value objects, the
//! `SystemSnapshot` aggregate, alert rules, and the persistence port.
//!
//! Zero I/O — collection, storage, and HTTP live in the application
//! and adapter layers.

pub mod alert;
pub mod bandwidth;
pub mod error;
pub mod metric;
pub mod repository;
pub mod snapshot;

pub use alert::{Alert, AlertRule};
pub use bandwidth::{
    BandwidthCounter, BandwidthObserver, BandwidthObserverFanout, BandwidthPeriod,
    BandwidthThresholdCrossed, BandwidthWindow, BandwidthWindowClosed, NoopBandwidthObserver,
};
pub use error::MonitoringError;
pub use metric::{MetricKind, MetricSample, Unit, clamp_to_percent};
pub use repository::SnapshotRepository;
pub use snapshot::{DiskReading, NetworkReading, SystemSnapshot};
