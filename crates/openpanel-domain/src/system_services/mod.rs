//! Allowlisted system-service descriptors and bounded health transitions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Service inventory validation failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ServiceError {
    /// Descriptor data is not safe or complete.
    #[error("invalid service descriptor")]
    InvalidDescriptor,
    /// Health transition threshold must be positive.
    #[error("invalid health threshold")]
    InvalidThreshold,
    /// Requested action is not registered for the service.
    #[error("unsupported service action")]
    UnsupportedAction,
    /// Disruptive action was not explicitly confirmed.
    #[error("service action requires confirmation")]
    ConfirmationRequired,
}

/// Stable request-facing descriptor ID; never a system unit name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceId(String);
impl ServiceId {
    /// Validate a lowercase slug.
    pub fn new(value: impl Into<String>) -> Result<Self, ServiceError> {
        let value = value.into();
        if value.is_empty()
            || value.len() > 64
            || !value
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
        {
            return Err(ServiceError::InvalidDescriptor);
        }
        Ok(Self(value))
    }

    /// Request-facing slug.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fixed lifecycle operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceAction {
    /// Activate the unit.
    Start,
    /// Deactivate the unit.
    Stop,
    /// Stop and start the unit.
    Restart,
    /// Reload configuration without a full restart.
    Reload,
    /// Enable startup activation.
    Enable,
    /// Disable startup activation.
    Disable,
}
impl ServiceAction {
    /// Whether an explicit impact confirmation is mandatory.
    pub fn is_disruptive(self) -> bool {
        matches!(self, Self::Stop | Self::Restart)
    }

    /// Fixed systemctl verb.
    pub fn verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Reload => "reload",
            Self::Enable => "enable",
            Self::Disable => "disable",
        }
    }
}

/// Composition-owned mapping from safe ID to fixed system unit.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceDescriptor {
    id: ServiceId,
    display_name: String,
    unit: String,
    actions: Vec<ServiceAction>,
    dependent_capabilities: Vec<String>,
}
impl ServiceDescriptor {
    /// Construct a fixed descriptor from trusted composition configuration.
    pub fn new(
        id: ServiceId,
        display_name: impl Into<String>,
        unit: impl Into<String>,
        actions: Vec<ServiceAction>,
        dependent_capabilities: Vec<String>,
    ) -> Result<Self, ServiceError> {
        let display_name = display_name.into();
        let unit = unit.into();
        if display_name.trim().is_empty()
            || !unit.ends_with(".service")
            || unit
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control() || matches!(ch, '/' | '\\' | ';'))
            || actions.is_empty()
            || dependent_capabilities
                .iter()
                .any(|value| value.is_empty() || value.chars().any(char::is_control))
        {
            return Err(ServiceError::InvalidDescriptor);
        }
        Ok(Self {
            id,
            display_name,
            unit,
            actions,
            dependent_capabilities,
        })
    }

    /// Stable request-facing identifier.
    pub fn id(&self) -> &ServiceId {
        &self.id
    }

    /// Operator-facing display name.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Fixed controller unit configured by composition.
    pub fn unit(&self) -> &str {
        &self.unit
    }

    /// Whether an action belongs to this descriptor's allowlist.
    pub fn supports(&self, action: ServiceAction) -> bool {
        self.actions.contains(&action)
    }

    /// Allowlisted lifecycle operations.
    pub fn actions(&self) -> &[ServiceAction] {
        &self.actions
    }

    /// OpenPanel capabilities affected by downtime.
    pub fn dependent_capabilities(&self) -> &[String] {
        &self.dependent_capabilities
    }

    /// Enforce action support and disruptive confirmation.
    pub fn validate_action(
        &self,
        action: ServiceAction,
        confirmed: bool,
    ) -> Result<(), ServiceError> {
        if !self.supports(action) {
            return Err(ServiceError::UnsupportedAction);
        }
        if action.is_disruptive() && !confirmed {
            return Err(ServiceError::ConfirmationRequired);
        }
        Ok(())
    }
}

/// Stabilized health exposed to operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    /// Not enough observations have been made.
    Unknown,
    /// Readiness is stably successful.
    Healthy,
    /// Readiness is stably failing.
    Failed,
}

/// Consecutive-observation hysteresis state machine.
pub struct HealthTracker {
    threshold: u32,
    consecutive: u32,
    observed_healthy: Option<bool>,
    health: Health,
}
impl HealthTracker {
    /// Construct with a positive consecutive-observation threshold.
    pub fn new(threshold: u32) -> Result<Self, ServiceError> {
        if threshold == 0 {
            return Err(ServiceError::InvalidThreshold);
        }
        Ok(Self {
            threshold,
            consecutive: 0,
            observed_healthy: None,
            health: Health::Unknown,
        })
    }

    /// Observe a probe and emit only stabilized transitions.
    pub fn observe(&mut self, healthy: bool) -> Option<Health> {
        if self.observed_healthy == Some(healthy) {
            self.consecutive = self.consecutive.saturating_add(1);
        } else {
            self.observed_healthy = Some(healthy);
            self.consecutive = 1;
        }
        if self.consecutive < self.threshold {
            return None;
        }
        let next = if healthy {
            Health::Healthy
        } else {
            Health::Failed
        };
        if next == self.health {
            None
        } else {
            self.health = next;
            Some(next)
        }
    }

    /// Current stabilized state.
    pub fn health(&self) -> Health {
        self.health
    }
}

/// Monotonic auto-restart attempt budget with a minimum cooldown.
pub struct AutoRestartBudget {
    maximum_attempts: u32,
    cooldown_seconds: u64,
    attempts: u32,
    last_attempt: Option<u64>,
}
impl AutoRestartBudget {
    /// Construct a positive attempt budget and cooldown.
    pub fn new(maximum_attempts: u32, cooldown_seconds: u64) -> Result<Self, ServiceError> {
        if maximum_attempts == 0 || cooldown_seconds == 0 {
            return Err(ServiceError::InvalidThreshold);
        }
        Ok(Self {
            maximum_attempts,
            cooldown_seconds,
            attempts: 0,
            last_attempt: None,
        })
    }

    /// Acquire one attempt when budget and cooldown permit it.
    pub fn try_acquire(&mut self, now_seconds: u64) -> bool {
        if self.attempts >= self.maximum_attempts
            || self
                .last_attempt
                .is_some_and(|last| now_seconds.saturating_sub(last) < self.cooldown_seconds)
        {
            return false;
        }
        self.attempts = self.attempts.saturating_add(1);
        self.last_attempt = Some(now_seconds);
        true
    }

    /// Clear attempts after stable recovery or a new policy window.
    pub fn reset(&mut self) {
        self.attempts = 0;
        self.last_attempt = None;
    }
}
