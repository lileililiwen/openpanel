//! Databases bounded context: MySQL provisioning with encrypted
//! credential storage.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! aggregate, value objects, repository **trait**, and error type.

/// The `Database` aggregate and its lifecycle operations.
pub mod database;
/// Supported database engines.
pub mod engine;
/// Errors returned by the databases bounded context.
pub mod error;
/// Persistence contract for databases.
pub mod repository;
/// The lifecycle status of a database.
pub mod status;

pub use database::Database;
pub use engine::DatabaseEngine;
pub use error::DatabaseError;
pub use repository::DatabaseRepository;
pub use status::DatabaseStatus;
