//! SQLite-backed adapter for the site-cache-cdn bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    CdnIntegration, CdnKind, PurgeReceipt, SiteCacheCdnError, SiteCacheCdnRepository,
    SiteCachePolicy,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed site-cache-cdn repository.
#[derive(Clone)]
pub struct SqliteSiteCacheCdnRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSiteCacheCdnRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn kind_str(kind: CdnKind) -> &'static str {
    kind.as_str()
}

fn kind_from_str(s: &str) -> CdnKind {
    match s {
        "cloudfront" => CdnKind::CloudFront,
        "generic_http" => CdnKind::GenericHttp,
        _ => CdnKind::Cloudflare,
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn parse_json_list(s: &str) -> Vec<String> {
    serde_json::from_str(s).unwrap_or_default()
}

#[async_trait]
impl SiteCacheCdnRepository for SqliteSiteCacheCdnRepository {
    async fn upsert_policy(&self, policy: &SiteCachePolicy) -> Result<(), SiteCacheCdnError> {
        sqlx::query(
            "INSERT OR REPLACE INTO site_cache_policies
             (site_id, ttl_seconds, bypass_paths, static_assets_ttl_seconds,
              keyed_cookies, stale_while_revalidate, revalidation_required, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(policy.site_id().to_string())
        .bind(policy.ttl_seconds() as i64)
        .bind(
            serde_json::to_string(policy.bypass_paths())
                .map_err(|e| SiteCacheCdnError::Persistence(format!("bypass json: {e}")))?,
        )
        .bind(policy.static_assets_ttl_seconds() as i64)
        .bind(
            serde_json::to_string(policy.keyed_cookies())
                .map_err(|e| SiteCacheCdnError::Persistence(format!("cookies json: {e}")))?,
        )
        .bind(policy.stale_while_revalidate() as i64)
        .bind(policy.revalidation_required() as i64)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("upsert policy: {e}")))?;
        Ok(())
    }

    async fn policy_for_site(
        &self,
        site_id: Uuid,
    ) -> Result<Option<SiteCachePolicy>, SiteCacheCdnError> {
        let row = sqlx::query(
            "SELECT site_id, ttl_seconds, bypass_paths, static_assets_ttl_seconds,
                    keyed_cookies, stale_while_revalidate, revalidation_required
             FROM site_cache_policies WHERE site_id = ?1",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("policy: {e}")))?;
        let Some(row) = row else {
            return Ok(None);
        };
        let mut policy =
            SiteCachePolicy::with_ttl(site_id, row.get::<i64, _>("ttl_seconds") as u32)
                .map_err(|e| SiteCacheCdnError::Persistence(format!("row decode: {e}")))?;
        let bypass: String = row.get("bypass_paths");
        let cookies: String = row.get("keyed_cookies");
        policy = policy
            .with_bypass_paths(parse_json_list(&bypass))
            .map_err(|e| SiteCacheCdnError::Persistence(format!("bypass decode: {e}")))?;
        policy = policy
            .with_keyed_cookies(parse_json_list(&cookies))
            .map_err(|e| SiteCacheCdnError::Persistence(format!("cookies decode: {e}")))?;
        policy = policy
            .with_static_assets_ttl(row.get::<i64, _>("static_assets_ttl_seconds") as u32)
            .map_err(|e| SiteCacheCdnError::Persistence(format!("static ttl decode: {e}")))?;
        policy =
            policy.with_stale_while_revalidate(row.get::<i64, _>("stale_while_revalidate") != 0);
        policy = policy.with_revalidation_required(row.get::<i64, _>("revalidation_required") != 0);
        Ok(Some(policy))
    }

    async fn insert_integration(
        &self,
        integration: &CdnIntegration,
    ) -> Result<(), SiteCacheCdnError> {
        sqlx::query(
            "INSERT INTO cdn_integrations (id, name, kind, config_enc, enabled, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(integration.id().to_string())
        .bind(integration.name())
        .bind(kind_str(integration.kind()))
        .bind(integration.config_enc())
        .bind(integration.enabled() as i64)
        .bind(integration.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("insert integration: {e}")))?;
        Ok(())
    }

    async fn find_integration(
        &self,
        id: Uuid,
    ) -> Result<Option<CdnIntegration>, SiteCacheCdnError> {
        let row = sqlx::query(
            "SELECT id, name, kind, config_enc, enabled, created_at
             FROM cdn_integrations WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("find integration: {e}")))?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(decode_integration(&row)?))
    }

    async fn list_integrations(&self) -> Result<Vec<CdnIntegration>, SiteCacheCdnError> {
        let rows = sqlx::query(
            "SELECT id, name, kind, config_enc, enabled, created_at
             FROM cdn_integrations ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("list integrations: {e}")))?;
        rows.into_iter()
            .map(|row| decode_integration(&row))
            .collect()
    }

    async fn delete_integration(&self, id: Uuid) -> Result<(), SiteCacheCdnError> {
        let res = sqlx::query("DELETE FROM cdn_integrations WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SiteCacheCdnError::Persistence(format!("delete integration: {e}")))?;
        if res.rows_affected() == 0 {
            return Err(SiteCacheCdnError::IntegrationNotFound);
        }
        Ok(())
    }

    async fn record_purge(
        &self,
        integration_id: Uuid,
        receipt: &PurgeReceipt,
    ) -> Result<(), SiteCacheCdnError> {
        sqlx::query(
            "INSERT INTO cdn_purge_log (integration_id, purged_json, at)
             VALUES (?1, ?2, ?3)",
        )
        .bind(integration_id.to_string())
        .bind(
            serde_json::to_string(receipt.purged())
                .map_err(|e| SiteCacheCdnError::Persistence(format!("purged json: {e}")))?,
        )
        .bind(receipt.at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("record purge: {e}")))?;
        Ok(())
    }

    async fn recent_purges(
        &self,
        integration_id: Uuid,
    ) -> Result<Vec<PurgeReceipt>, SiteCacheCdnError> {
        let rows = sqlx::query(
            "SELECT purged_json, at FROM cdn_purge_log
             WHERE integration_id = ?1 ORDER BY at DESC LIMIT 50",
        )
        .bind(integration_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| SiteCacheCdnError::Persistence(format!("recent purges: {e}")))?;
        rows.into_iter()
            .map(|row| {
                let purged: String = row.get("purged_json");
                let at: String = row.get("at");
                Ok(PurgeReceipt::new(parse_json_list(&purged), parse_ts(&at)))
            })
            .collect()
    }
}

fn decode_integration(row: &sqlx::sqlite::SqliteRow) -> Result<CdnIntegration, SiteCacheCdnError> {
    let id: String = row.get("id");
    let name: String = row.get("name");
    let kind: String = row.get("kind");
    let config_enc: String = row.get("config_enc");
    let enabled: i64 = row.get("enabled");
    let created_at: String = row.get("created_at");
    let mut integration = CdnIntegration::new(
        Uuid::parse_str(&id).map_err(|e| SiteCacheCdnError::Persistence(format!("bad id: {e}")))?,
        name,
        kind_from_str(&kind),
        config_enc,
        parse_ts(&created_at),
    )
    .map_err(|e| SiteCacheCdnError::Persistence(format!("row decode: {e}")))?;
    integration.set_enabled(enabled != 0);
    Ok(integration)
}
