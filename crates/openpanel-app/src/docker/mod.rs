//! Container-management application boundary and adapters.

mod adapter;
mod module;
mod repo;
mod service;
mod task;

pub use adapter::BollardDockerAdapter;
pub use module::DockerModule;
pub use repo::SqliteDockerRepository;
pub use service::{
    ApplyReport, DockerAdapter, DockerService, ExecResult, NetworkAdapter, RuntimeContainerState,
};
pub use task::DockerReconcileTask;
