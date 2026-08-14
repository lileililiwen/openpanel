//! Log viewer application layer: in-memory JSONL store,
//! SQLite-backed download repository, aggregator service, and
//! module composition.

mod module;
mod repo;
mod service;
#[cfg(test)]
mod tests;

pub use module::{LogViewerModule, MODULE_NAME};
pub use repo::SqliteLogViewerRepository;
pub use service::{InMemoryLogReader, LogAggregator};
