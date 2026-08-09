//! Safe allowlisted host-service orchestration.

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role,
    logs::redact_log_text,
    system_services::{
        AutoRestartBudget, Health, HealthTracker, ServiceAction, ServiceDescriptor, ServiceError,
    },
};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

mod adapters;
mod module;
mod repo;
pub use adapters::*;
pub use module::*;
pub use repo::*;

/// Controller-observed service state.
#[derive(Debug, Clone, Serialize)]
pub struct ControllerStatus {
    /// systemd load state.
    pub load_state: String,
    /// systemd active state.
    pub active_state: String,
    /// systemd sub state.
    pub sub_state: String,
    /// Whether startup enablement is active.
    pub enabled: bool,
}
impl ControllerStatus {
    /// Deterministic active state for tests and fake controllers.
    pub fn active() -> Self {
        Self {
            load_state: "loaded".into(),
            active_state: "active".into(),
            sub_state: "running".into(),
            enabled: true,
        }
    }
}

/// Fixed-argv privileged service boundary.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ServiceController: Send + Sync {
    /// Refresh actual state for a fixed composition-owned unit.
    async fn status(&self, unit: &str) -> Result<ControllerStatus, ServiceManagerError>;
    /// Perform one typed action for a fixed composition-owned unit.
    async fn action(&self, unit: &str, action: ServiceAction) -> Result<(), ServiceManagerError>;
    /// Check post-action readiness.
    async fn probe(&self, unit: &str) -> Result<bool, ServiceManagerError>;
}

/// Bounded journal boundary.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait JournalPort: Send + Sync {
    /// Read at most `limit` entries for a fixed unit.
    async fn entries(&self, unit: &str, limit: usize) -> Result<Vec<String>, ServiceManagerError>;
}

/// Durable health-history boundary.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ServiceHealthRepository: Send + Sync {
    /// Return bounded recent stabilized health values.
    async fn history(
        &self,
        service_id: &str,
        limit: usize,
    ) -> Result<Vec<Health>, ServiceManagerError>;
    /// Persist one stabilized health transition.
    async fn record(&self, service_id: &str, health: Health) -> Result<(), ServiceManagerError>;
}

/// Periodic hysteresis, transition alert, and opt-in restart supervisor.
pub struct HealthSupervisor {
    descriptors: Vec<ServiceDescriptor>,
    controller: Arc<dyn ServiceController>,
    repository: Arc<dyn ServiceHealthRepository>,
    audit: Arc<dyn AuditService>,
    trackers: std::sync::Mutex<HashMap<String, HealthTracker>>,
    budgets: std::sync::Mutex<HashMap<String, AutoRestartBudget>>,
    auto_restart: bool,
}
impl HealthSupervisor {
    /// Build deterministic per-service trackers and bounded restart budgets.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        descriptors: Vec<ServiceDescriptor>,
        controller: Arc<dyn ServiceController>,
        repository: Arc<dyn ServiceHealthRepository>,
        audit: Arc<dyn AuditService>,
        threshold: u32,
        auto_restart: bool,
        max_attempts: u32,
        cooldown_seconds: u64,
    ) -> Result<Self, ServiceManagerError> {
        let mut trackers = HashMap::new();
        let mut budgets = HashMap::new();
        for descriptor in &descriptors {
            trackers.insert(
                descriptor.id().as_str().to_string(),
                HealthTracker::new(threshold).map_err(|_| ServiceManagerError::Controller)?,
            );
            budgets.insert(
                descriptor.id().as_str().to_string(),
                AutoRestartBudget::new(max_attempts, cooldown_seconds)
                    .map_err(|_| ServiceManagerError::Controller)?,
            );
        }
        Ok(Self {
            descriptors,
            controller,
            repository,
            audit,
            trackers: std::sync::Mutex::new(trackers),
            budgets: std::sync::Mutex::new(budgets),
            auto_restart,
        })
    }

    /// Poll every registered service once and emit stabilized transitions.
    pub async fn tick(&self, now_seconds: u64) -> Result<(), ServiceManagerError> {
        for descriptor in &self.descriptors {
            let healthy = self.controller.probe(descriptor.unit()).await?;
            let transition = {
                let mut trackers = self
                    .trackers
                    .lock()
                    .map_err(|_| ServiceManagerError::Persistence)?;
                trackers
                    .get_mut(descriptor.id().as_str())
                    .and_then(|tracker| tracker.observe(healthy))
            };
            if let Some(health) = transition {
                self.repository
                    .record(descriptor.id().as_str(), health)
                    .await?;
                let _ = self
                    .audit
                    .record(
                        AuditEvent::new("system", AuditAction::AlertFired, AuditOutcome::Success)
                            .target(descriptor.id().as_str())
                            .metadata(serde_json::json!({"health":health})),
                    )
                    .await;
                if health == Health::Failed && self.auto_restart {
                    let acquired = {
                        let mut budgets = self
                            .budgets
                            .lock()
                            .map_err(|_| ServiceManagerError::Persistence)?;
                        budgets
                            .get_mut(descriptor.id().as_str())
                            .is_some_and(|budget| budget.try_acquire(now_seconds))
                    };
                    if acquired {
                        self.controller
                            .action(descriptor.unit(), ServiceAction::Restart)
                            .await?;
                    }
                } else if health == Health::Healthy {
                    let mut budgets = self
                        .budgets
                        .lock()
                        .map_err(|_| ServiceManagerError::Persistence)?;
                    if let Some(budget) = budgets.get_mut(descriptor.id().as_str()) {
                        budget.reset();
                    }
                }
            }
        }
        Ok(())
    }
}

/// Redacted service-use-case failure.
#[derive(Debug, Error)]
pub enum ServiceManagerError {
    /// Descriptor ID is not registered.
    #[error("service not found")]
    NotFound,
    /// Caller role cannot perform the action.
    #[error("service action forbidden")]
    Forbidden,
    /// Disruptive action lacks confirmation.
    #[error("service action requires confirmation")]
    ConfirmationRequired,
    /// Descriptor does not allow the action.
    #[error("unsupported service action")]
    Unsupported,
    /// Controller, probe, or journal failed safely.
    #[error("service controller unavailable")]
    Controller,
    /// Post-action readiness did not recover.
    #[error("service readiness failed")]
    Readiness,
    /// Durable history failed.
    #[error("service persistence unavailable")]
    Persistence,
}

/// Descriptor plus live controller state.
#[derive(Debug, Clone, Serialize)]
pub struct ManagedServiceStatus {
    /// Safe descriptor.
    #[serde(flatten)]
    pub descriptor: ServiceDescriptor,
    /// Refreshed host state.
    #[serde(flatten)]
    pub status: ControllerStatus,
}

/// Impact-only disruptive action preview.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceImpact {
    /// Requested typed action.
    pub action: ServiceAction,
    /// Capabilities that may be interrupted.
    pub dependent_capabilities: Vec<String>,
    /// Whether confirmation is required.
    pub confirmation_required: bool,
}

/// Safe lifecycle, status, health, and journal use cases.
pub struct ServiceManager {
    descriptors: HashMap<String, ServiceDescriptor>,
    controller: Arc<dyn ServiceController>,
    journal: Arc<dyn JournalPort>,
    health: Arc<dyn ServiceHealthRepository>,
    audit: Arc<dyn AuditService>,
}
impl ServiceManager {
    /// Compose over a fixed descriptor inventory and explicit ports.
    pub fn new(
        descriptors: Vec<ServiceDescriptor>,
        controller: Arc<dyn ServiceController>,
        journal: Arc<dyn JournalPort>,
        health: Arc<dyn ServiceHealthRepository>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            descriptors: descriptors
                .into_iter()
                .map(|descriptor| (descriptor.id().as_str().to_string(), descriptor))
                .collect(),
            controller,
            journal,
            health,
            audit,
        }
    }

    fn descriptor(&self, id: &str) -> Result<&ServiceDescriptor, ServiceManagerError> {
        self.descriptors
            .get(id)
            .ok_or(ServiceManagerError::NotFound)
    }

    /// List registered services with refreshed actual state.
    pub async fn inventory(&self) -> Result<Vec<ManagedServiceStatus>, ServiceManagerError> {
        let mut values = Vec::with_capacity(self.descriptors.len());
        for descriptor in self.descriptors.values() {
            values.push(ManagedServiceStatus {
                descriptor: descriptor.clone(),
                status: self.controller.status(descriptor.unit()).await?,
            });
        }
        values.sort_by(|left, right| {
            left.descriptor
                .id()
                .as_str()
                .cmp(right.descriptor.id().as_str())
        });
        Ok(values)
    }

    /// Refresh one registered service.
    pub async fn status(&self, id: &str) -> Result<ManagedServiceStatus, ServiceManagerError> {
        let descriptor = self.descriptor(id)?.clone();
        Ok(ManagedServiceStatus {
            status: self.controller.status(descriptor.unit()).await?,
            descriptor,
        })
    }

    /// Calculate action impact without host mutation.
    pub fn preview(
        &self,
        id: &str,
        action: ServiceAction,
    ) -> Result<ServiceImpact, ServiceManagerError> {
        let descriptor = self.descriptor(id)?;
        if !descriptor.supports(action) {
            return Err(ServiceManagerError::Unsupported);
        }
        Ok(ServiceImpact {
            action,
            dependent_capabilities: descriptor.dependent_capabilities().to_vec(),
            confirmation_required: action.is_disruptive(),
        })
    }

    /// Authorize, execute, probe readiness, refresh actual state, and audit.
    pub async fn perform(
        &self,
        actor: Uuid,
        role: Role,
        id: &str,
        action: ServiceAction,
        confirmed: bool,
    ) -> Result<ManagedServiceStatus, ServiceManagerError> {
        let descriptor = self.descriptor(id)?.clone();
        if !matches!(role, Role::Owner)
            && !(matches!(role, Role::Admin) && action == ServiceAction::Reload)
        {
            return Err(ServiceManagerError::Forbidden);
        }
        descriptor
            .validate_action(action, confirmed)
            .map_err(map_domain)?;
        self.controller.action(descriptor.unit(), action).await?;
        if matches!(
            action,
            ServiceAction::Start | ServiceAction::Restart | ServiceAction::Reload
        ) && !self.controller.probe(descriptor.unit()).await?
        {
            return Err(ServiceManagerError::Readiness);
        }
        let result = ManagedServiceStatus {
            status: self.controller.status(descriptor.unit()).await?,
            descriptor,
        };
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::ServiceChanged,
                    AuditOutcome::Success,
                )
                .target(id)
                .metadata(serde_json::json!({"action":action})),
            )
            .await;
        Ok(result)
    }

    /// Return bounded health history.
    pub async fn history(
        &self,
        id: &str,
        limit: usize,
    ) -> Result<Vec<Health>, ServiceManagerError> {
        self.descriptor(id)?;
        self.health.history(id, limit.min(500)).await
    }

    /// Return bounded redacted journal entries.
    pub async fn logs(&self, id: &str, limit: usize) -> Result<Vec<String>, ServiceManagerError> {
        let descriptor = self.descriptor(id)?;
        Ok(self
            .journal
            .entries(descriptor.unit(), limit.clamp(1, 500))
            .await?
            .into_iter()
            .map(|line| redact_log_text(&line))
            .collect())
    }
}
fn map_domain(error: ServiceError) -> ServiceManagerError {
    match error {
        ServiceError::ConfirmationRequired => ServiceManagerError::ConfirmationRequired,
        ServiceError::UnsupportedAction => ServiceManagerError::Unsupported,
        _ => ServiceManagerError::Controller,
    }
}
