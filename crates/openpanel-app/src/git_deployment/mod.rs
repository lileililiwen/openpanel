//! Git deployment application layer: SQLite repository, deploy
//! service, webhook verifier, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{GitDeploymentModule, MODULE_NAME};
pub use repo::SqliteDeployRepository;
pub use service::{DeployService, WebhookVerifier, verify_webhook_with_secret};
