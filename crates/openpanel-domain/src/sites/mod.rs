//! Sites bounded context: nginx vhost provisioning.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! aggregate, value objects, repository **trait**, and error type.

/// Error type for the sites bounded context.
pub mod error;
/// Site aggregate persistence trait implemented in `openpanel-app`.
pub mod repository;
/// Site aggregate: domain, aliases, document root, status.
pub mod site;
/// Lifecycle status of a site.
pub mod status;
pub mod transport;

pub use error::SiteError;
pub use repository::SiteRepository;
pub use site::Site;
pub use status::SiteStatus;
pub use transport::{ByteSize, CompressionPolicy, HstsPolicy, TlsVersion, TransportPolicy};
