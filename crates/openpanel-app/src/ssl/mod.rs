//! ssl bounded context: ACME issuance, manual upload, self-signed,
//! auto-renewal, nginx TLS integration.
//!
//! Zero I/O at the module level — composition root wires the service,
//! the challenge server, the renewal task, and the migration.

pub mod acme;
pub mod challenge_server;
pub mod crypto;
pub mod module;
pub mod parser;
pub mod renewal;
pub mod repo;
pub mod self_signed;
pub mod service;

pub use acme::{AcmeClient, AcmeEndpoint, IssuedCert};
pub use challenge_server::AcmeHttpServer;
pub use module::{MODULE_NAME, SslModule};
pub use parser::NewCertMaterial;
pub use renewal::SslRenewalTask;
pub use repo::SqliteCertificateRepository;
pub use service::{SslPaths, SslService};
