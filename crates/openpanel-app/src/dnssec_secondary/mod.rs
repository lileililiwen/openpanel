//! DNSSEC + secondary DNS application layer: SQLite repository,
//! signing service, key rollover, AXFR sender, glue service.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{DnsSecSecondaryModule, MODULE_NAME};
pub use repo::SqliteDnsSecRepository;
pub use service::{
    AxfrSender, DnsSecService, GlueRecordService, KeyRolloverEngine, Registrar,
    RecordingRegistrar,
};