use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    RepoError,
    sites::{site::Site, status::SiteStatus},
};

/// Persistence contract for sites. Implementations live in
/// `openpanel-app` and decide the storage engine.
#[async_trait]
pub trait SiteRepository: Send + Sync + 'static {
    /// Persist a new site. Returns an error if the primary domain
    /// already exists.
    async fn insert(&self, site: &Site) -> Result<(), RepoError>;

    /// Find a site by its id, or `None` if it does not exist.
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Site>, RepoError>;

    /// Find a site by its primary domain, or `None` if it does not exist.
    async fn find_by_domain(&self, domain: &str) -> Result<Option<Site>, RepoError>;

    /// List every site.
    async fn list_all(&self) -> Result<Vec<Site>, RepoError>;

    /// List every site owned by the given user.
    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Site>, RepoError>;

    /// Update a site's status, recording `by` as the modifier.
    async fn update_status(&self, id: Uuid, status: SiteStatus, by: &str) -> Result<(), RepoError>;

    /// Transfer a site to a new owner, recording `by` as the modifier.
    async fn update_owner(&self, id: Uuid, owner_id: Uuid, by: &str) -> Result<(), RepoError>;

    /// Replace a site's aliases from JSON, recording `by` as the modifier.
    async fn update_aliases(&self, id: Uuid, aliases_json: &str, by: &str)
    -> Result<(), RepoError>;

    /// Delete a site by its id.
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;

    /// Return the total number of sites.
    async fn count(&self) -> Result<i64, RepoError>;
}
