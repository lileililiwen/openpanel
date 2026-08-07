//! Files bounded context: chrooted file manager for sites.
//!
//! All I/O lives in `openpanel-app`. This crate contributes only the
//! path value object, file info read model, error type, and repository
//! **trait**.

pub mod error;
pub mod file_info;
pub mod path;
pub mod repository;

pub use error::FileError;
pub use file_info::FileInfo;
pub use path::Path;
pub use repository::FileRepository;