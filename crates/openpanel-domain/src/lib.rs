//! Domain layer: entities, value objects, aggregates, repository traits.
//! Zero I/O — no sqlx, no axum, no tokio. Implementation lives in
//! `openpanel-app`.

#![deny(rustdoc::broken_intra_doc_links)]
// Workspace lints deny `unwrap_used` / `expect_used` / `panic` in
// production. Test code (`#[cfg(test)]`) MAY contain these, so we relax
// the deny to a warn for test compilation only.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod common;
/// Scheduled command and HTTP job domain model.
pub mod cron;
pub mod databases;
pub mod files;
pub mod identity;
pub mod monitoring;
pub mod sites;
pub mod ssl;

pub use common::{
    Email, Password, PasswordError, Username, UsernameError,
    error::{DomainError, RepoError},
};
pub use databases::{
    database::Database, engine::DatabaseEngine, error::DatabaseError,
    repository::DatabaseRepository, status::DatabaseStatus,
};
pub use files::{
    error::FileError,
    file_info::FileInfo,
    path::Path,
    repository::{FileRepository, MAX_READ_BYTES},
};
pub use identity::{
    error::IdentityError,
    repository::{SessionRepository, UserRepository},
    role::Role,
    session::{Session, SessionBuilder, SessionToken, SessionTokenError},
    user::User,
};
pub use monitoring::{
    Alert, AlertRule, DiskReading, MetricKind, MetricSample, MonitoringError, NetworkReading,
    SnapshotRepository, SystemSnapshot, Unit,
};
pub use sites::{error::SiteError, repository::SiteRepository, site::Site, status::SiteStatus};
pub use ssl::{
    certificate::{Certificate, KeyType},
    error::SslError,
    repository::CertificateRepository,
    source::{CertificateSource, CertificateStatus},
};
