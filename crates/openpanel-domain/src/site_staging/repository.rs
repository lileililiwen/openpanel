//! Repository traits for the `site-staging` bounded context.
//!
//! The `StagingSlotRepository` persists the per-site slot; the
//! `SnapshotRepository` records each snapshot taken; the
//! `PromotionRepository` records the promotion runs.

use async_trait::async_trait;
use uuid::Uuid;

use crate::site_staging::{
    error::SiteStagingError,
    promotion::PromotionRun,
    slot::{SnapshotId, StagingSlot},
};

/// Persistence contract for `StagingSlot` aggregates.
#[async_trait]
pub trait StagingSlotRepository: Send + Sync + 'static {
    /// Insert a new slot.
    async fn insert(&self, slot: &StagingSlot) -> Result<(), SiteStagingError>;
    /// Look up by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<StagingSlot>, SiteStagingError>;
    /// Look up by site id (at most one slot per site).
    async fn find_by_site(&self, site_id: Uuid) -> Result<Option<StagingSlot>, SiteStagingError>;
    /// List every slot.
    async fn list_all(&self) -> Result<Vec<StagingSlot>, SiteStagingError>;
    /// Persist mutable fields.
    async fn update(&self, slot: &StagingSlot) -> Result<(), SiteStagingError>;
    /// Remove the slot.
    async fn delete(&self, id: Uuid) -> Result<(), SiteStagingError>;
}

/// Persistence contract for snapshot records.
#[async_trait]
pub trait SnapshotRepository: Send + Sync + 'static {
    /// Insert a new snapshot row.
    async fn insert(
        &self,
        site_id: Uuid,
        snapshot: SnapshotId,
        taken_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), SiteStagingError>;
    /// List snapshots for a site, newest first.
    async fn list_by_site(&self, site_id: Uuid) -> Result<Vec<SnapshotRow>, SiteStagingError>;
}

/// Minimal snapshot projection returned by `SnapshotRepository::list_by_site`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotRow {
    pub site_id: Uuid,
    pub snapshot: SnapshotId,
    pub taken_at: chrono::DateTime<chrono::Utc>,
}

/// Persistence contract for `PromotionRun` aggregates.
#[async_trait]
pub trait PromotionRepository: Send + Sync + 'static {
    /// Insert a new promotion run.
    async fn insert(&self, run: &PromotionRun) -> Result<(), SiteStagingError>;
    /// Look up by id.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<PromotionRun>, SiteStagingError>;
    /// List every promotion for a site, newest first.
    async fn list_by_site(&self, site_id: Uuid) -> Result<Vec<PromotionRun>, SiteStagingError>;
    /// Persist status / failure-reason changes.
    async fn update(&self, run: &PromotionRun) -> Result<(), SiteStagingError>;
}
