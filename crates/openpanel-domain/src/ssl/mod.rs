//! TLS bounded context: certificate lifecycle, ACME issuance,
//! manual upload, self-signed generation, automatic renewal.
//!
//! This module owns the `Certificate` aggregate, the
//! `CertificateRepository` trait, the `CertificateSource` /
//! `CertificateStatus` enums, and the `SslError` error type. Zero I/O —
//! no sqlx, no axum, no tokio. Implementation lives in `openpanel-app`.

#![deny(rustdoc::broken_intra_doc_links)]

pub mod certificate;
pub mod error;
pub mod repository;
pub mod source;

pub use certificate::{Certificate, KeyType};
pub use error::SslError;
pub use repository::CertificateRepository;
pub use source::{CertificateSource, CertificateStatus};
