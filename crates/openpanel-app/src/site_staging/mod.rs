//! Per-site staging bounded context: staging slot creation, sync,
//! and atomic promote.
//!
//! The application service orchestrates the staging flow:
//!
//! * `create_slot` / `delete_slot` — manage the per-site slot.
//! * `sync_snapshot` — copy production files and DB into staging,
//!   advancing the snapshot id.
//! * `promote` — atomic rename chain + nginx reload; rolls back
//!   on failure.
//!
//! Filesystem operations are abstracted behind a
//! [`StagingFilesystemLayer`] trait so production code can
//! shell out to `rsync` / `nginx` while tests run entirely
//! in-memory.

pub mod files;
pub mod in_memory;
pub mod module;
pub mod repo;
pub mod service;

pub use files::{InMemoryStagingFilesystem, StagingFilesystemLayer};
pub use in_memory::InMemoryPromotionNotifier;
pub use module::{MODULE_NAME, SiteStagingModule};
pub use openpanel_domain::SiteStagingError;
pub use service::{CreateSlotRequest, PromotionServiceError, StagingService};
