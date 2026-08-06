//! Sites bounded context: nginx vhost provisioning.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! aggregate, value objects, repository **trait**, and error type.

pub mod error;
pub mod repository;
pub mod site;
pub mod status;

pub use error::SiteError;
pub use repository::SiteRepository;
pub use site::Site;
pub use status::SiteStatus;