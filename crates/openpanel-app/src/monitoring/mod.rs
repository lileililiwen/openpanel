//! Monitoring bounded context — application layer.
//!
//! Collection (`sysinfo`-backed), persistence (SQLite time series),
//! alert evaluation, and the background collector task.

pub mod alerts;
pub mod collector;
pub mod module;
pub mod repo;
pub mod service;
pub mod task;

pub use alerts::{AlertConfig, AlertEvaluator};
pub use collector::{Collector, SystemCollector};
pub use module::{MODULE_NAME, MonitoringModule};
pub use service::MonitoringService;
pub use task::MonitoringCollectorTask;
