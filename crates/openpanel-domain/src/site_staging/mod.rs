//! Per-site staging bounded context.
//!
//! A `StagingSlot` lives at a configurable subdomain of the
//! production site (default `staging.<primary_domain>`), with a
//! document root under `/var/www/<domain>/staging/public_html` and
//! a separate MySQL database `<owner>_<site>_staging`. The slot
//! supports three sync policies (OnDemand, OnPromote, Scheduled),
//! an audit-friendly snapshot id, and an atomic promote that swaps
//! the production and staging document roots under a single rename
//! chain with nginx reload in between.

pub mod error;
pub mod promotion;
pub mod repository;
pub mod slot;

pub use error::SiteStagingError;
pub use promotion::{PromotionRun, PromotionStatus};
pub use repository::{
    PromotionRepository, SnapshotRepository as StagingSnapshotRepository, SnapshotRow,
    StagingSlotRepository,
};
pub use slot::{SnapshotId, StagingSlot, SyncMode, SyncPolicy};
