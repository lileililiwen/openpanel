//! ssl bounded context: ACME issuance, manual upload, self-signed,
//! auto-renewal, nginx TLS integration.
//!
//! Zero I/O at the module level — composition root wires the service,
//! the challenge server, the renewal task, and the migration.

pub mod acme;
pub mod challenge_server;
pub mod crypto;
pub mod issuance_state;
pub mod module;
pub mod parser;
pub mod preflight;
pub mod renewal;
pub mod repo;
pub mod self_signed;
pub mod service;
pub mod tests;

pub use acme::{AcmeClient, AcmeEndpoint, IssuedCert};
pub use challenge_server::AcmeHttpServer;
pub use issuance_state::{
    INITIAL_POLL_BACKOFF, IssuanceAttempt, IssuanceError, MAX_POLL_ATTEMPTS, MAX_POLL_BACKOFF,
    RENEWAL_RETRY_AFTER, classify_acme_error, classify_problem, redact_acme_text,
};
pub use module::{MODULE_NAME, SslModule};
pub use parser::NewCertMaterial;
pub use preflight::{PREFLIGHT_CHALLENGE_PORT, PREFLIGHT_HTTP_PORT, PreflightOutcome, preflight};
pub use renewal::SslRenewalTask;
pub use repo::SqliteCertificateRepository;
pub use service::{SslPaths, SslService};
