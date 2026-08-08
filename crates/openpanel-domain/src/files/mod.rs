//! Files bounded context: chrooted file manager for sites.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! path value object, file info read model, error type, and repository
//! **trait**.

/// Error type for the files bounded context.
pub mod error;
/// Read-model describing a single entry in a site's document root.
pub mod file_info;
/// Validated relative path inside a site's document root.
pub mod path;
/// File-system access trait implemented in `openpanel-app`.
pub mod repository;

pub use error::FileError;
pub use file_info::FileInfo;
pub use path::Path;
pub use repository::FileRepository;
