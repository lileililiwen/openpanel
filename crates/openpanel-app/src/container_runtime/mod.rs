//! Container runtime bounded context: per-user quota, registry
//! credentials (encrypted at rest under the master key),
//! per-container metrics, and monthly network egress
//! accounting. Built on top of the `docker` bounded context.
//!
//! The service stays within its own bounded context — it does
//! not import `openpanel_app::docker`; instead the docker
//! service is the *consumer* of the quota gate (the docker
//! service asks `ContainerRuntimeService::check_quota` before
//! every container create).

pub mod crypto;
pub mod metrics_poller;
pub mod module;
pub mod registry_adapter;
pub mod repo;
pub mod service;
#[cfg(test)]
mod service_tests;

pub use metrics_poller::MetricsPollerTask;
pub use module::{ContainerRuntimeModule, MODULE_NAME};
pub use registry_adapter::{
    NoopRegistryHostAdapter, PullError, PullRequest, PullResult, RegistryHostAdapter,
};
pub use repo::SqliteContainerRuntimeRepository;
pub use service::{
    ContainerRuntimeService, CreateCredentialResult, ProposedContainer, QuotaDecision, QuotaUpdate,
    UsageSnapshot,
};
