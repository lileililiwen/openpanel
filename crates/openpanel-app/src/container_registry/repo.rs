//! SQLite-backed repositories for the container registry.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    common::error::RepoError,
    container_registry::{
        image::{ImageDigest, ScanStatus, StoredImage},
        namespace::{ImageNamespace, NamespaceId},
        scan::{ScanFinding, ScanResult},
    },
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

fn parse_status(s: &str) -> Result<ScanStatus, RepoError> {
    match s {
        "pending" => Ok(ScanStatus::Pending),
        "clean" => Ok(ScanStatus::Clean),
        "with_findings" => Ok(ScanStatus::WithFindings),
        "failed" => Ok(ScanStatus::Failed),
        other => Err(RepoError::new(format!("unknown scan status `{other}`"))),
    }
}

fn status_str(s: ScanStatus) -> &'static str {
    match s {
        ScanStatus::Pending => "pending",
        ScanStatus::Clean => "clean",
        ScanStatus::WithFindings => "with_findings",
        ScanStatus::Failed => "failed",
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, RepoError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| RepoError::new(format!("invalid datetime `{s}`: {e}")))
}

// ---- Image namespace ----

/// SQLite-backed image-namespace repository.
#[derive(Clone)]
pub struct SqliteNamespaceRepository {
    pool: Pool<Sqlite>,
}

impl SqliteNamespaceRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

/// Port for persisting image namespaces.
#[async_trait]
pub trait NamespaceRepository: Send + Sync {
    /// Insert or replace a namespace row.
    async fn insert(&self, ns: &ImageNamespace) -> Result<(), RepoError>;
    /// Look up a namespace by id.
    async fn find(&self, id: &NamespaceId) -> Result<Option<ImageNamespace>, RepoError>;
    /// Update the used-bytes counter for a namespace.
    async fn update_used(&self, id: &NamespaceId, used: u64) -> Result<(), RepoError>;
    /// List all namespaces, oldest first.
    async fn list(&self) -> Result<Vec<ImageNamespace>, RepoError>;
}

#[async_trait]
impl NamespaceRepository for SqliteNamespaceRepository {
    async fn insert(&self, ns: &ImageNamespace) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO image_namespaces
                (namespace_id, owner, quota_bytes, used_bytes, created_at)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ns.namespace_id.as_str())
        .bind(ns.owner.to_string())
        .bind(ns.quota_bytes as i64)
        .bind(ns.used_bytes as i64)
        .bind(ns.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find(&self, id: &NamespaceId) -> Result<Option<ImageNamespace>, RepoError> {
        let row: Option<NamespaceRow> = sqlx::query_as(
            "SELECT namespace_id, owner, quota_bytes, used_bytes, created_at
             FROM image_namespaces WHERE namespace_id = ?",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_namespace).transpose()
    }

    async fn update_used(&self, id: &NamespaceId, used: u64) -> Result<(), RepoError> {
        let res = sqlx::query("UPDATE image_namespaces SET used_bytes = ? WHERE namespace_id = ?")
            .bind(used as i64)
            .bind(id.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!("namespace {id} not found")));
        }
        Ok(())
    }

    async fn list(&self) -> Result<Vec<ImageNamespace>, RepoError> {
        let rows: Vec<NamespaceRow> = sqlx::query_as(
            "SELECT namespace_id, owner, quota_bytes, used_bytes, created_at
             FROM image_namespaces ORDER BY created_at ASC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_namespace).collect()
    }
}

#[derive(sqlx::FromRow)]
struct NamespaceRow {
    namespace_id: String,
    owner: String,
    quota_bytes: i64,
    used_bytes: i64,
    created_at: String,
}

fn row_to_namespace(row: NamespaceRow) -> Result<ImageNamespace, RepoError> {
    let owner = Uuid::parse_str(&row.owner).map_err(|e| RepoError::new(format!("owner: {e}")))?;
    Ok(ImageNamespace {
        namespace_id: NamespaceId::new(row.namespace_id)
            .map_err(|e| RepoError::new(e.to_string()))?,
        owner,
        quota_bytes: row.quota_bytes as u64,
        used_bytes: row.used_bytes as u64,
        created_at: parse_dt(&row.created_at)?,
    })
}

// ---- Stored images ----

/// SQLite-backed stored-image repository.
#[derive(Clone)]
pub struct SqliteImageRepository {
    pool: Pool<Sqlite>,
}

impl SqliteImageRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

/// Port for persisting stored images.
#[async_trait]
pub trait ImageRepository: Send + Sync {
    /// Insert or replace a stored-image row.
    async fn insert(&self, image: &StoredImage) -> Result<(), RepoError>;
    /// Look up an image in a namespace by digest.
    async fn find(
        &self,
        namespace: &NamespaceId,
        digest: &ImageDigest,
    ) -> Result<Option<StoredImage>, RepoError>;
    /// Delete a stored-image row.
    async fn delete(&self, namespace: &NamespaceId, digest: &ImageDigest) -> Result<(), RepoError>;
    /// List images in a namespace, oldest first.
    async fn list_for_namespace(
        &self,
        namespace: &NamespaceId,
    ) -> Result<Vec<StoredImage>, RepoError>;
    /// Update the scan status of an image.
    async fn update_status(
        &self,
        namespace: &NamespaceId,
        digest: &ImageDigest,
        status: ScanStatus,
    ) -> Result<(), RepoError>;
}

#[async_trait]
impl ImageRepository for SqliteImageRepository {
    async fn insert(&self, img: &StoredImage) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT OR REPLACE INTO stored_images
                (digest, namespace, size_bytes, pushed_at, reference, scan_status)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(img.digest.as_str())
        .bind(img.namespace.as_str())
        .bind(img.size_bytes as i64)
        .bind(img.pushed_at.to_rfc3339())
        .bind(img.reference.clone())
        .bind(status_str(img.scan_status))
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn find(
        &self,
        namespace: &NamespaceId,
        digest: &ImageDigest,
    ) -> Result<Option<StoredImage>, RepoError> {
        let row: Option<ImageRow> = sqlx::query_as(
            "SELECT digest, namespace, size_bytes, pushed_at, reference, scan_status
             FROM stored_images WHERE namespace = ? AND digest = ?",
        )
        .bind(namespace.as_str())
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_image).transpose()
    }

    async fn delete(&self, namespace: &NamespaceId, digest: &ImageDigest) -> Result<(), RepoError> {
        sqlx::query("DELETE FROM stored_images WHERE namespace = ? AND digest = ?")
            .bind(namespace.as_str())
            .bind(digest.as_str())
            .execute(&self.pool)
            .await
            .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn list_for_namespace(
        &self,
        namespace: &NamespaceId,
    ) -> Result<Vec<StoredImage>, RepoError> {
        let rows: Vec<ImageRow> = sqlx::query_as(
            "SELECT digest, namespace, size_bytes, pushed_at, reference, scan_status
             FROM stored_images WHERE namespace = ?
             ORDER BY pushed_at ASC",
        )
        .bind(namespace.as_str())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        rows.into_iter().map(row_to_image).collect()
    }

    async fn update_status(
        &self,
        namespace: &NamespaceId,
        digest: &ImageDigest,
        status: ScanStatus,
    ) -> Result<(), RepoError> {
        let res = sqlx::query(
            "UPDATE stored_images SET scan_status = ?
             WHERE namespace = ? AND digest = ?",
        )
        .bind(status_str(status))
        .bind(namespace.as_str())
        .bind(digest.as_str())
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        if res.rows_affected() == 0 {
            return Err(RepoError::new(format!(
                "image {digest} not found in namespace {namespace}"
            )));
        }
        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct ImageRow {
    digest: String,
    namespace: String,
    size_bytes: i64,
    pushed_at: String,
    reference: Option<String>,
    scan_status: String,
}

fn row_to_image(row: ImageRow) -> Result<StoredImage, RepoError> {
    Ok(StoredImage {
        digest: ImageDigest::new(row.digest).map_err(|e| RepoError::new(e.to_string()))?,
        namespace: NamespaceId::new(row.namespace).map_err(|e| RepoError::new(e.to_string()))?,
        size_bytes: row.size_bytes as u64,
        pushed_at: parse_dt(&row.pushed_at)?,
        reference: row.reference,
        scan_status: parse_status(&row.scan_status)?,
    })
}

// ---- Scan results ----

/// SQLite-backed scan-result repository.
#[derive(Clone)]
pub struct SqliteScanResultRepository {
    pool: Pool<Sqlite>,
}

impl SqliteScanResultRepository {
    /// Construct a new repository bound to `pool`.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

/// Port for persisting scan results.
#[async_trait]
pub trait ScanResultRepository: Send + Sync {
    /// Insert a scan result row.
    async fn insert(&self, result: &ScanResult) -> Result<(), RepoError>;
    /// Latest scan result for an image digest (if any).
    async fn latest_for(&self, digest: &ImageDigest) -> Result<Option<ScanResult>, RepoError>;
}

#[async_trait]
impl ScanResultRepository for SqliteScanResultRepository {
    async fn insert(&self, r: &ScanResult) -> Result<(), RepoError> {
        sqlx::query(
            "INSERT INTO scan_results
                (digest, scanned_at, findings_json)
             VALUES (?, ?, ?)",
        )
        .bind(r.digest.as_str())
        .bind(r.scanned_at.to_rfc3339())
        .bind(serde_json::to_string(&r.findings).map_err(|e| RepoError::new(e.to_string()))?)
        .execute(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        Ok(())
    }

    async fn latest_for(&self, digest: &ImageDigest) -> Result<Option<ScanResult>, RepoError> {
        let row: Option<ScanResultRow> = sqlx::query_as(
            "SELECT digest, scanned_at, findings_json
             FROM scan_results WHERE digest = ?
             ORDER BY scanned_at DESC LIMIT 1",
        )
        .bind(digest.as_str())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| RepoError::new(e.to_string()))?;
        row.map(row_to_scan_result).transpose()
    }
}

#[derive(sqlx::FromRow)]
struct ScanResultRow {
    digest: String,
    scanned_at: String,
    findings_json: String,
}

fn row_to_scan_result(row: ScanResultRow) -> Result<ScanResult, RepoError> {
    let findings: Vec<ScanFinding> =
        serde_json::from_str(&row.findings_json).map_err(|e| RepoError::new(e.to_string()))?;
    Ok(ScanResult {
        digest: ImageDigest::new(row.digest).map_err(|e| RepoError::new(e.to_string()))?,
        scanned_at: parse_dt(&row.scanned_at)?,
        findings,
    })
}
