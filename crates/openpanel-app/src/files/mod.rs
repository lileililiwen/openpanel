//! Files bounded context: use cases + filesystem-backed repository.

pub mod module;
pub mod repo;
pub mod service;

pub use module::FilesModule;
pub use repo::FilesystemRepository;
pub use service::FilesService;