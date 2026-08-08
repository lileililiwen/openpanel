//! Error type for the monitoring bounded context. Pure domain — no I/O.

use thiserror::Error;

/// All failures surfaced by the monitoring bounded context.
#[derive(Debug, PartialEq, Error)]
pub enum MonitoringError {
    /// The requested snapshot / metric has no data.
    #[error("no monitoring data for `{0}`")]
    NotFound(String),

    /// The repository rejected the operation (unique violation, etc.).
    #[error("monitoring repository error: {0}")]
    Repo(String),

    /// A metric kind was not recognized.
    #[error("unknown metric kind: `{0}`")]
    InvalidKind(String),

    /// A sample value was not finite (NaN / infinity).
    #[error("metric value must be finite, got `{0}`")]
    InvalidValue(String),

    /// The snapshot's timestamp is in the future.
    #[error("snapshot timestamp cannot be in the future")]
    TimestampInFuture,

    /// A snapshot must contain at least one sample.
    #[error("snapshot must contain at least one sample")]
    EmptySnapshot,

    /// A generic I/O failure (reading host metrics, writing rows).
    #[error("I/O error: {0}")]
    Io(String),
}
