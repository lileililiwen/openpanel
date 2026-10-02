//! Deployment adapters application layer: the typed
//! `DeploymentAdapter` service, and composition module. The
//! domain types and the trait live in
//! `openpanel_domain::deployment_adapters`; this layer owns the
//! lifecycle, idempotency, evidence persistence, audit fan-out,
//! and the composition module that wires the bounded context
//! into the registry.
//!
//! No conformance adapter ships in this crate beyond the production
//! docker delivery adapter: each other adapter is provided by its
//! owning bounded context, the integration test suite, or an
//! external orchestrator. The Mac/Jenkins conformance fixture lives
//! in `tests/integration/deployment_adapters.rs` and is a regression
//! target for the adapter contract — it is not a product
//! dependency.

mod docker_adapter;
#[cfg(test)]
mod docker_adapter_tests;
mod service;
mod types;

pub use docker_adapter::{DOCKER_DEPLOYMENT_ADAPTER_ID, DockerDeploymentAdapter};
pub use service::DeploymentAdapterService;
pub use types::DeploymentServiceError;
