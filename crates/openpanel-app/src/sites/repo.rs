//! SQLite-backed adapter for `SiteRepository`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::Pool;
use sqlx::Sqlite;
use uuid::Uuid;

use openpanel_domain::sites::site::Site;
use openpanel_domain::sites::status::SiteStatus;
use openpanel_domain::{RepoError, SiteRepository};

#[derive(Clone)]
pub struct SqliteSiteRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSiteRepository {
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl SiteRepository for SqliteSiteRepository {
    async fn insert(&self, site: &Site) -> Result<(), RepoError> {
        let aliases_json = serde_json::to_string(site.aliases())
            .map_err(|e| RepoError::new(e.to_string()))?;
        sqlx::query(
            r#"
            INSERT INTO sites
                (id, owner_id, primary_domain, aliases, document_root,
                 php_enabled, php_version, status, created_at, updated_at,
                 created_by, modified_by)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(site.id().to_string())
        .bind(site.owner_id().to_string())
        .bind(site.primary_domain())
        .bind(aliases_json)
        .bind(site.document_root())
        .bind(site.php_enabled())
        .bind(site.php_version())
        .bind(site.status().as_str())
        .bind(site.created_at().to_rfc3339())
        .bind(site.updated_at().to_rfc3339())
        .bind(site.created_by())
        .bind(site.modified_by())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<Site>, RepoError> {
        let row: Option<SiteRow> = sqlx::query_as::<_, SiteRow>(
            "SELECT id, owner_id, primary_domain, aliases, document_root,
                    php_enabled, php_version, status, created_at, updated_at,
                    created_by, modified_by FROM sites WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(SiteRow::into_site).transpose()
    }

    async fn find_by_domain(&self, domain: &str) -> Result<Option<Site>, RepoError> {
        let row: Option<SiteRow> = sqlx::query_as::<_, SiteRow>(
            "SELECT id, owner_id, primary_domain, aliases, document_root,
                    php_enabled, php_version, status, created_at, updated_at,
                    created_by, modified_by FROM sites WHERE primary_domain = ?",
        )
        .bind(domain.to_lowercase())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(SiteRow::into_site).transpose()
    }

    async fn list_all(&self) -> Result<Vec<Site>, RepoError> {
        let rows: Vec<SiteRow> = sqlx::query_as::<_, SiteRow>(
            "SELECT id, owner_id, primary_domain, aliases, document_root,
                    php_enabled, php_version, status, created_at, updated_at,
                    created_by, modified_by FROM sites ORDER BY primary_domain",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(SiteRow::into_site).collect()
    }

    async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Site>, RepoError> {
        let rows: Vec<SiteRow> = sqlx::query_as::<_, SiteRow>(
            "SELECT id, owner_id, primary_domain, aliases, document_root,
                    php_enabled, php_version, status, created_at, updated_at,
                    created_by, modified_by FROM sites WHERE owner_id = ? ORDER BY primary_domain",
        )
        .bind(owner_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(SiteRow::into_site).collect()
    }

    async fn update_status(
        &self,
        id: Uuid,
        status: SiteStatus,
        by: &str,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE sites SET status = ?, updated_at = ?, modified_by = ? WHERE id = ?")
            .bind(status.as_str())
            .bind(Utc::now().to_rfc3339())
            .bind(by)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_owner(&self, id: Uuid, owner_id: Uuid, by: &str) -> Result<(), RepoError> {
        sqlx::query("UPDATE sites SET owner_id = ?, updated_at = ?, modified_by = ? WHERE id = ?")
            .bind(owner_id.to_string())
            .bind(Utc::now().to_rfc3339())
            .bind(by)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn update_aliases(
        &self,
        id: Uuid,
        aliases_json: &str,
        by: &str,
    ) -> Result<(), RepoError> {
        sqlx::query("UPDATE sites SET aliases = ?, updated_at = ?, modified_by = ? WHERE id = ?")
            .bind(aliases_json)
            .bind(Utc::now().to_rfc3339())
            .bind(by)
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM sites WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn count(&self) -> Result<i64, RepoError> {
        let n = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sites")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(n)
    }
}

#[derive(sqlx::FromRow)]
struct SiteRow {
    id: String,
    owner_id: String,
    primary_domain: String,
    aliases: String,
    document_root: String,
    php_enabled: bool,
    php_version: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
    created_by: String,
    modified_by: String,
}

impl SiteRow {
    fn into_site(self) -> Result<Site, RepoError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| RepoError::new(format!("bad site id: {e}")))?;
        let owner_id = Uuid::parse_str(&self.owner_id)
            .map_err(|e| RepoError::new(format!("bad owner id: {e}")))?;
        let aliases: Vec<String> = serde_json::from_str(&self.aliases)
            .map_err(|e| RepoError::new(format!("bad aliases json: {e}")))?;
        let status: SiteStatus = self
            .status
            .parse()
            .map_err(|e: String| RepoError::new(format!("bad status: {e}")))?;
        let created_at = parse_dt(&self.created_at)?;
        let updated_at = parse_dt(&self.updated_at)?;
        Ok(Site::restore(
            id,
            owner_id,
            self.primary_domain,
            aliases,
            self.document_root,
            self.php_enabled,
            self.php_version,
            status,
            created_at,
            updated_at,
            self.created_by,
            self.modified_by,
        ))
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("bad timestamp `{s}`: {e}")))
}