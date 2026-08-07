//! Databases bounded context: MySQL provisioning with encrypted
//! credential storage.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! aggregate, value objects, repository **trait**, and error type.

pub mod database;
pub mod engine;
pub mod error;
pub mod repository;
pub mod status;

pub use database::Database;
pub use engine::DatabaseEngine;
pub use error::DatabaseError;
pub use repository::DatabaseRepository;
pub use status::DatabaseStatus;
