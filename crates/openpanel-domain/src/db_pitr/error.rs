//! Point-in-time recovery domain errors.

use thiserror::Error;

/// Domain errors returned by the `db-pitr` bounded context.
///
/// The error variants are typed so callers can distinguish between
/// "user supplied an invalid timestamp", "the chosen restore target
/// is missing", and "an I/O adapter failed" without parsing strings.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PitrError {
    /// A value failed domain validation.
    #[error("invalid PITR value: {0}")]
    Invalid(String),

    /// A lifecycle transition is not permitted.
    #[error("invalid PITR transition")]
    InvalidTransition,

    /// The restore target (full backup or binlog segment) is missing.
    #[error("restore target missing: {0}")]
    TargetMissing(String),

    /// The caller chose a timestamp outside the available replay window.
    #[error("timestamp {0} is outside the available replay window")]
    TimestampOutOfRange(String),

    /// The streaming engine or sink reported an I/O failure.
    #[error("binlog I/O error: {0}")]
    Io(String),
}
