//! Backup restore drill domain model.
//!
//! A restore drill runs the existing restore pipeline against a
//! sandboxed environment and evaluates assertions to verify backup
//! integrity without touching production data.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Domain validation and transition failures for restore drills.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum DrillError {
    /// A value is invalid.
    #[error("invalid drill value: {0}")]
    Invalid(String),
    /// The requested lifecycle transition is not allowed.
    #[error("invalid drill transition: {0}")]
    InvalidTransition(String),
}

/// The outcome of a completed restore drill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrillOutcome {
    /// All assertions passed.
    Passed,
    /// At least one assertion failed.
    Failed,
}

/// The kind of assertion evaluated during a drill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrillAssertionKind {
    /// Database dump applies and contains tables.
    Database,
    /// Site archive extracts with valid manifest.
    Site,
    /// SSL key ciphertext decrypts under local master key.
    SslKeys,
    /// Panel metadata manifest is valid.
    PanelMetadata,
}

/// Result of a single assertion evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillAssertion {
    /// What was checked.
    pub kind: DrillAssertionKind,
    /// Whether the assertion passed.
    pub passed: bool,
    /// Human-readable detail (error message on failure, summary on pass).
    pub detail: String,
}

impl DrillAssertion {
    /// Create a passing assertion.
    pub fn passed(kind: DrillAssertionKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            passed: true,
            detail: detail.into(),
        }
    }

    /// Create a failing assertion.
    pub fn failed(kind: DrillAssertionKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            passed: false,
            detail: detail.into(),
        }
    }
}

/// Lifecycle state of a restore drill.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DrillState {
    /// Drill is executing.
    Running,
    /// Drill completed — all assertions passed.
    Passed,
    /// Drill completed — at least one assertion failed.
    Failed,
}

/// A restore drill aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestoreDrill {
    /// Unique identifier.
    pub id: Uuid,
    /// The backup run being drilled.
    pub backup_run_id: Uuid,
    /// Current lifecycle state.
    pub state: DrillState,
    /// Assertion results (populated on completion).
    pub assertions: Vec<DrillAssertion>,
    /// When the drill started.
    pub started_at: DateTime<Utc>,
    /// When the drill completed (None if still running).
    pub completed_at: Option<DateTime<Utc>>,
}

impl RestoreDrill {
    /// Start a new drill.
    pub fn start(backup_run_id: Uuid, now: DateTime<Utc>) -> Self {
        Self {
            id: Uuid::new_v4(),
            backup_run_id,
            state: DrillState::Running,
            assertions: Vec::new(),
            started_at: now,
            completed_at: None,
        }
    }

    /// Finish the drill with the given outcome and assertions.
    ///
    /// # Errors
    ///
    /// Returns `DrillError::InvalidTransition` if the drill is not in
    /// the `Running` state.
    pub fn finish(
        mut self,
        outcome: DrillOutcome,
        assertions: Vec<DrillAssertion>,
        now: DateTime<Utc>,
    ) -> Result<Self, DrillError> {
        if self.state != DrillState::Running {
            return Err(DrillError::InvalidTransition(format!(
                "cannot finish drill in state {:?}",
                self.state
            )));
        }
        self.state = match outcome {
            DrillOutcome::Passed => DrillState::Passed,
            DrillOutcome::Failed => DrillState::Failed,
        };
        self.assertions = assertions;
        self.completed_at = Some(now);
        Ok(self)
    }

    /// Derive the outcome from the current state.
    pub fn outcome(&self) -> Option<DrillOutcome> {
        match self.state {
            DrillState::Running => None,
            DrillState::Passed => Some(DrillOutcome::Passed),
            DrillState::Failed => Some(DrillOutcome::Failed),
        }
    }
}

/// Maximum number of completed drills to retain (default keep).
pub const DEFAULT_DRILL_RETENTION: usize = 20;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_creates_running_drill() {
        let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
        assert_eq!(drill.state, DrillState::Running);
        assert!(drill.assertions.is_empty());
        assert!(drill.completed_at.is_none());
        assert_eq!(drill.outcome(), None);
    }

    #[test]
    fn finish_running_to_passed() {
        let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
        let assertions = vec![DrillAssertion::passed(
            DrillAssertionKind::Database,
            "3 tables found",
        )];
        let finished = drill
            .finish(DrillOutcome::Passed, assertions.clone(), Utc::now())
            .unwrap();
        assert_eq!(finished.state, DrillState::Passed);
        assert_eq!(finished.assertions, assertions);
        assert!(finished.completed_at.is_some());
        assert_eq!(finished.outcome(), Some(DrillOutcome::Passed));
    }

    #[test]
    fn finish_running_to_failed() {
        let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
        let assertions = vec![DrillAssertion::failed(
            DrillAssertionKind::Site,
            "manifest missing",
        )];
        let finished = drill
            .finish(DrillOutcome::Failed, assertions, Utc::now())
            .unwrap();
        assert_eq!(finished.state, DrillState::Failed);
        assert_eq!(finished.outcome(), Some(DrillOutcome::Failed));
    }

    #[test]
    fn finish_twice_rejected() {
        let drill = RestoreDrill::start(Uuid::new_v4(), Utc::now());
        let finished = drill
            .finish(DrillOutcome::Passed, vec![], Utc::now())
            .unwrap();
        let result = finished.finish(DrillOutcome::Failed, vec![], Utc::now());
        assert!(result.is_err());
        match result.unwrap_err() {
            DrillError::InvalidTransition(msg) => {
                assert!(!msg.is_empty(), "expected non-empty transition error");
            }
            other => panic!("expected InvalidTransition, got {other:?}"),
        }
    }

    #[test]
    fn assertion_passed_construction() {
        let a = DrillAssertion::passed(DrillAssertionKind::SslKeys, "decrypted ok");
        assert!(a.passed);
        assert_eq!(a.detail, "decrypted ok");
    }

    #[test]
    fn assertion_failed_construction() {
        let a = DrillAssertion::failed(DrillAssertionKind::PanelMetadata, "schema mismatch");
        assert!(!a.passed);
        assert_eq!(a.detail, "schema mismatch");
    }
}
