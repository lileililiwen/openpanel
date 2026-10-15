//! Staging mail-capture bounded context: capture service, inbox
//! persistence with encrypted bodies at rest, and module wiring.

pub mod module;
pub mod repo;
pub mod service;

pub use module::MailCaptureModule;
pub use repo::SqliteMailCaptureRepository;
pub use service::{
    MailCaptureService, MailCaptureView, RETENTION_CAP_PER_SLOT, ReadCapturedMessageView,
};
