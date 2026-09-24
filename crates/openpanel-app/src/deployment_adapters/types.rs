//! Application errors with safe public messages for the
//! deployment-adapter service.

use openpanel_domain::deployment_adapters::DeploymentAdapterError;
use thiserror::Error;

/// Application failures from the deployment-adapter service. Each
/// variant carries a safe public message; never the secret value,
/// never the host path.
#[derive(Debug, Error)]
pub enum DeploymentServiceError {
    /// Caller lacks permission to run deployment actions.
    #[error("forbidden")]
    Forbidden,
    /// The named adapter is not registered.
    #[error("adapter not found: {0}")]
    AdapterNotFound(String),
    /// Domain validation rejected the input.
    #[error("deployment-adapter validation failed: {0}")]
    Validation(String),
    /// The plan asks for an action the adapter does not declare.
    #[error("unsupported adapter action: {0}")]
    UnsupportedAction(&'static str),
    /// The plan asks for rollback but the adapter does not
    /// declare support.
    #[error("rollback is not supported by this adapter")]
    RollbackUnsupported,
    /// Persistence or transport failure (caller-safe message).
    #[error("deployment-adapter service unavailable")]
    Internal,
}

impl From<DeploymentAdapterError> for DeploymentServiceError {
    fn from(error: DeploymentAdapterError) -> Self {
        match error {
            DeploymentAdapterError::UnsupportedAction(action) => Self::UnsupportedAction(action),
            other => Self::Validation(other.to_string()),
        }
    }
}
