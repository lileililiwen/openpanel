//! Wildcard SSL application layer: SQLite repository, challenge
//! solver, wildcard issuer, and module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{MODULE_NAME, WildcardSslModule};
pub use repo::SqliteWildcardRepository;
pub use service::{
    CertRenewalScheduler, Dns01ChallengeSolver, DnsProviderPort, WildcardIssuer,
};
pub use openpanel_domain::RecordingDnsProvider;
