//! IPv6 + address-pool application layer: SQLite repository,
//! allocator, vhost binder, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{IpAllocationModule, MODULE_NAME};
pub use repo::SqliteIpRepository;
pub use service::{Allocator, IpService, VhostBinder, candidate_addresses};
