//! Git deployment application layer: SQLite repository, deploy
//! service, webhook verifier, and module composition.

mod module;
mod preview_repo;
mod preview_service;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{GitDeploymentModule, MODULE_NAME};
pub use preview_repo::SqlitePreviewRepository;
pub use preview_service::PreviewService;
pub use repo::SqliteDeployRepository;
pub use service::{DeployService, WebhookVerifier, verify_webhook_with_secret};
