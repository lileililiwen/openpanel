//! Synthetic monitoring bounded context: periodic HTTP / TCP / SSL
//! checks with a per-check throttle and a typed alert decision.
//!
//! The runner records the result of every probe; a `CheckResult`
//! is then classified as `Ok` / `Warn` / `Fail` against the
//! check's thresholds. The classification drives the alert
//! emission pipeline; the runner itself never raises a raw
//! network error to the user.

pub mod status_page;

pub use status_page::{
    DailyBar, Incident, Slug, StatusEntry, StatusPage, StatusPageError, StatusPageRepository,
    derive_incidents, uptime_bars_90d,
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised by the synthetic monitoring bounded context.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyntheticError {
    /// The caller is not authorised.
    #[error("forbidden")]
    Forbidden,
    /// The check target is malformed.
    #[error("invalid target: {0}")]
    InvalidTarget(String),
    /// The check id does not exist.
    #[error("check not found: {0}")]
    CheckNotFound(Uuid),
    /// The throttle window has not elapsed since the last run.
    #[error("throttled")]
    Throttled,
    /// Probe execution failed.
    #[error("probe failed: {0}")]
    Probe(String),
    /// Persistence failed.
    #[error("persistence failed: {0}")]
    Persistence(String),
}

impl From<SyntheticError> for RepoError {
    fn from(error: SyntheticError) -> Self {
        RepoError::new(error.to_string())
    }
}

impl From<RepoError> for SyntheticError {
    fn from(error: RepoError) -> Self {
        SyntheticError::Persistence(error.0)
    }
}

/// Type of a synthetic check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckType {
    /// HTTP GET with a status-code predicate.
    Http,
    /// TCP connect with a timeout.
    Tcp,
    /// TLS handshake + certificate expiry inspection.
    Ssl,
}

impl CheckType {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Tcp => "tcp",
            Self::Ssl => "ssl",
        }
    }

    /// Parse a stored label back into the enum.
    pub fn from_label(label: &str) -> Option<Self> {
        match label {
            "http" => Some(Self::Http),
            "tcp" => Some(Self::Tcp),
            "ssl" => Some(Self::Ssl),
            _ => None,
        }
    }
}

/// Result classification. `Warn` is used for soft signals (slow
/// response, certificate inside the warn window); `Fail` is the
/// hard signal that drives a notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    /// The check passed cleanly.
    Ok,
    /// The check passed but the soft threshold was crossed.
    Warn,
    /// The check failed.
    Fail,
}

impl CheckStatus {
    /// Stable lower-case label.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }
}

/// A configured synthetic check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyntheticCheck {
    /// Stable id.
    pub id: Uuid,
    /// Human-readable name.
    pub name: String,
    /// Check type.
    pub kind: CheckType,
    /// Target (URL, host:port, or hostname for SSL).
    pub target: String,
    /// Optional expected HTTP status (for HTTP checks).
    #[serde(default)]
    pub expected_status: Option<u16>,
    /// Connection timeout in seconds.
    pub timeout_secs: u32,
    /// Throttle window in seconds — minimum interval between runs.
    pub throttle_secs: u32,
    /// SSL: number of days before expiry to raise a `Warn`.
    pub warn_before_days: u32,
    /// When the check was created.
    pub created_at: DateTime<Utc>,
    /// When the check last ran.
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    /// Whether the check is enabled.
    pub enabled: bool,
}

impl SyntheticCheck {
    /// Validate the check's target and timeouts.
    pub fn validate(&self) -> Result<(), SyntheticError> {
        if self.target.trim().is_empty() {
            return Err(SyntheticError::InvalidTarget("empty target".into()));
        }
        if self.timeout_secs == 0 || self.timeout_secs > 120 {
            return Err(SyntheticError::InvalidTarget(
                "timeout_secs must be in 1..=120".into(),
            ));
        }
        if self.throttle_secs > 86_400 {
            return Err(SyntheticError::InvalidTarget(
                "throttle_secs must be <= 86400".into(),
            ));
        }
        match self.kind {
            CheckType::Http => {
                if !(self.target.starts_with("http://") || self.target.starts_with("https://")) {
                    return Err(SyntheticError::InvalidTarget(
                        "http target must start with http:// or https://".into(),
                    ));
                }
            }
            CheckType::Tcp => {
                if !self.target.contains(':') {
                    return Err(SyntheticError::InvalidTarget(
                        "tcp target must be host:port".into(),
                    ));
                }
            }
            CheckType::Ssl => {
                if self.target.contains('/') {
                    return Err(SyntheticError::InvalidTarget(
                        "ssl target must be a hostname".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Recorded result of a single probe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckResult {
    /// Stable id.
    pub id: Uuid,
    /// Check that produced the result.
    pub check_id: Uuid,
    /// When the probe ran.
    pub ran_at: DateTime<Utc>,
    /// Latency in milliseconds.
    pub latency_ms: u32,
    /// HTTP status (for HTTP checks).
    #[serde(default)]
    pub http_status: Option<u16>,
    /// Days until certificate expiry (for SSL checks).
    #[serde(default)]
    pub cert_days_remaining: Option<u32>,
    /// Final classification.
    pub status: CheckStatus,
    /// Optional error / detail string.
    #[serde(default)]
    pub message: String,
}

/// Persistence port for the synthetic monitoring bounded context.
#[async_trait]
pub trait SyntheticRepository: Send + Sync + 'static {
    /// Persist a check.
    async fn save_check(&self, check: &SyntheticCheck) -> Result<(), RepoError>;
    /// Load a check by id.
    async fn get_check(&self, id: Uuid) -> Result<Option<SyntheticCheck>, RepoError>;
    /// List all checks.
    async fn list_checks(&self) -> Result<Vec<SyntheticCheck>, RepoError>;
    /// Update the `last_run_at` field of a check.
    async fn touch_check(&self, id: Uuid, ran_at: DateTime<Utc>) -> Result<(), RepoError>;
    /// Delete a check.
    async fn delete_check(&self, id: Uuid) -> Result<(), RepoError>;

    /// Persist a check result.
    async fn save_result(&self, result: &CheckResult) -> Result<(), RepoError>;
    /// List recent results for a check.
    async fn list_results(&self, check_id: Uuid, limit: u32)
    -> Result<Vec<CheckResult>, RepoError>;
}

/// Classify a probe into a `CheckStatus` given the configured
/// thresholds. Pure function — the test harness calls it directly
/// to assert the alert boundaries.
pub fn classify(status: CheckStatus, warn_threshold: CheckStatus) -> CheckStatus {
    // The `warn_threshold` argument lets the caller widen the
    // soft signal (e.g. a 5xx is Fail, a 4xx is Warn, anything
    // else is Ok). The threshold itself is the floor; the
    // returned status is `max(probe, threshold)` in the ordering
    // Ok < Warn < Fail.
    let order = |s: CheckStatus| match s {
        CheckStatus::Ok => 0,
        CheckStatus::Warn => 1,
        CheckStatus::Fail => 2,
    };
    if order(status) >= order(warn_threshold) {
        status
    } else {
        warn_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_uses_threshold_floor() {
        assert_eq!(
            classify(CheckStatus::Ok, CheckStatus::Warn),
            CheckStatus::Warn
        );
        assert_eq!(
            classify(CheckStatus::Warn, CheckStatus::Warn),
            CheckStatus::Warn
        );
        assert_eq!(
            classify(CheckStatus::Fail, CheckStatus::Warn),
            CheckStatus::Fail
        );
    }
}
