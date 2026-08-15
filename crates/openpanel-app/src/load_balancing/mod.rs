//! Load balancing and failover application layer: SQLite
//! repository, member rotator, health probe pipeline, and
//! module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{LoadBalancingModule, MODULE_NAME};
pub use repo::SqliteLbRepository;
pub use service::{LbService, MemberRotator, RecordingHealthProbe, RotationDecision};
