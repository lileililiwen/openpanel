use async_trait::async_trait;
use uuid::Uuid;

use crate::sites::site::Site;
use crate::sites::status::SiteStatus;
use crate::RepoError;

#[async_trait]
pub trait SiteRepository: Send + Sync + 'static {
    async fn insert(&self, site: &Site) -> Result<(), RepoError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Site>, RepoError>;
    async fn find_by_domain(&self, domain: &str) -> Result<Option<Site>, RepoError>;
    async fn list_all(&self) -> Result<Vec<Site>, RepoError>;
    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Site>, RepoError>;
    async fn update_status(
        &self,
        id: Uuid,
        status: SiteStatus,
        by: &str,
    ) -> Result<(), RepoError>;
    async fn update_owner(&self, id: Uuid, owner_id: Uuid, by: &str) -> Result<(), RepoError>;
    async fn update_aliases(&self, id: Uuid, aliases_json: &str, by: &str)
        -> Result<(), RepoError>;
    async fn delete(&self, id: Uuid) -> Result<(), RepoError>;
    async fn count(&self) -> Result<i64, RepoError>;
}