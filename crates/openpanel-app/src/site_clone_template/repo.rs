//! SQLite-backed adapter for the site-clone-template bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    AnonymisationToken, CloneFile, ClonePlan, CloneRun, CloneSource, DbAction, PiiPolicy,
    SiteCloneTemplateError, SiteCloneTemplateRepository, SiteTemplate,
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite-backed repository.
#[derive(Clone)]
pub struct SqliteSiteCloneTemplateRepository {
    pool: Pool<Sqlite>,
}

impl SqliteSiteCloneTemplateRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

fn parse_ts(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn encode_source(source: CloneSource) -> (String, String, Option<i64>) {
    match source {
        CloneSource::Site { site_id } => ("site".to_string(), site_id.to_string(), None),
        CloneSource::Snapshot {
            site_id,
            snapshot_id,
        } => (
            "snapshot".to_string(),
            site_id.to_string(),
            Some(snapshot_id),
        ),
        CloneSource::Template { template_id } => {
            ("template".to_string(), template_id.to_string(), None)
        }
    }
}

fn decode_source(
    kind: &str,
    id: &str,
    snapshot_id: Option<i64>,
) -> Result<CloneSource, SiteCloneTemplateError> {
    let id = Uuid::parse_str(id)
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("source id: {e}")))?;
    match kind {
        "site" => Ok(CloneSource::Site { site_id: id }),
        "snapshot" => Ok(CloneSource::Snapshot {
            site_id: id,
            snapshot_id: snapshot_id.unwrap_or(0),
        }),
        "template" => Ok(CloneSource::Template { template_id: id }),
        other => Err(SiteCloneTemplateError::Persistence(format!(
            "unknown source kind `{other}`"
        ))),
    }
}

#[async_trait]
impl SiteCloneTemplateRepository for SqliteSiteCloneTemplateRepository {
    async fn upsert_plan(&self, plan: &ClonePlan) -> Result<(), SiteCloneTemplateError> {
        let (kind, source_id, snapshot_id) = encode_source(plan.source());
        let files_json = serde_json::to_string(plan.files())
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("files: {e}")))?;
        let db_json = serde_json::to_string(plan.db())
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("db: {e}")))?;
        let warnings_json = serde_json::to_string(plan.warnings())
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("warnings: {e}")))?;
        sqlx::query(
            "INSERT OR REPLACE INTO clone_plans
             (id, source_kind, source_id, snapshot_id, target_owner_id, target_domain,
              pii_policy, files_json, db_action_json, warnings_json, content_hash,
              created_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )
        .bind(plan.id().to_string())
        .bind(kind)
        .bind(source_id)
        .bind(snapshot_id)
        .bind(plan.target_owner_id().to_string())
        .bind(plan.target_domain())
        .bind(plan.pii_policy().as_str())
        .bind(files_json)
        .bind(db_json)
        .bind(warnings_json)
        .bind(plan.content_hash())
        .bind(plan.created_at().to_rfc3339())
        .bind(plan.expires_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("upsert plan: {e}")))?;
        Ok(())
    }

    async fn find_plan(&self, id: Uuid) -> Result<Option<ClonePlan>, SiteCloneTemplateError> {
        let row = sqlx::query(
            "SELECT id, source_kind, source_id, snapshot_id, target_owner_id, target_domain,
                    pii_policy, files_json, db_action_json, warnings_json, content_hash,
                    created_at
             FROM clone_plans WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("find plan: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let source_kind: String = row.get("source_kind");
        let source_id: String = row.get("source_id");
        let snapshot_id: Option<i64> = row.get("snapshot_id");
        let source = decode_source(&source_kind, &source_id, snapshot_id)?;
        let target_owner_id: String = row.get("target_owner_id");
        let target_domain: String = row.get("target_domain");
        let pii_policy: String = row.get("pii_policy");
        let files_json: String = row.get("files_json");
        let db_json: String = row.get("db_action_json");
        let warnings_json: String = row.get("warnings_json");
        let content_hash: String = row.get("content_hash");
        let created_at: String = row.get("created_at");
        let owner = Uuid::parse_str(&target_owner_id)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("owner: {e}")))?;
        let policy = match pii_policy.as_str() {
            "keep" => PiiPolicy::Keep,
            "none" => PiiPolicy::None,
            _ => PiiPolicy::Standard,
        };
        let files: Vec<CloneFile> = serde_json::from_str(&files_json)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("files: {e}")))?;
        let db: DbAction = serde_json::from_str(&db_json)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("db: {e}")))?;
        let warnings: Vec<String> = serde_json::from_str(&warnings_json)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("warnings: {e}")))?;
        let plan = ClonePlan::new(
            id,
            source,
            owner,
            target_domain,
            policy,
            files,
            db,
            warnings,
            content_hash,
            parse_ts(&created_at),
        )
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("plan: {e}")))?;
        Ok(Some(plan))
    }

    async fn insert_run(&self, run: &CloneRun) -> Result<(), SiteCloneTemplateError> {
        let (kind, source_id, snapshot_id) = encode_source(run.source());
        sqlx::query(
            "INSERT INTO clone_runs
             (id, plan_id, target_site_id, source_kind, source_id, snapshot_id,
              pii_policy, started_at, finished_at, failure_reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
        )
        .bind(run.id().to_string())
        .bind(run.plan_id().to_string())
        .bind(run.target_site_id().to_string())
        .bind(kind)
        .bind(source_id)
        .bind(snapshot_id)
        .bind(run.pii_policy().as_str())
        .bind(run.started_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("insert run: {e}")))?;
        Ok(())
    }

    async fn finish_run(
        &self,
        run_id: Uuid,
        finished_at: DateTime<Utc>,
        failure_reason: Option<String>,
    ) -> Result<(), SiteCloneTemplateError> {
        sqlx::query("UPDATE clone_runs SET finished_at = ?1, failure_reason = ?2 WHERE id = ?3")
            .bind(finished_at.to_rfc3339())
            .bind(failure_reason)
            .bind(run_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("finish run: {e}")))?;
        Ok(())
    }

    async fn insert_template(&self, template: &SiteTemplate) -> Result<(), SiteCloneTemplateError> {
        let warnings_json = serde_json::to_string(template.warnings())
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("warnings: {e}")))?;
        sqlx::query(
            "INSERT INTO site_templates
             (id, name, source_site_id, artifact_path, signature, signature_valid,
              warnings, pii_policy, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )
        .bind(template.id().to_string())
        .bind(template.name())
        .bind(template.source_site_id().to_string())
        .bind(template.artifact_path())
        .bind(template.signature())
        .bind(template.signature_valid() as i64)
        .bind(warnings_json)
        .bind(template.pii_policy().as_str())
        .bind(template.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("insert template: {e}")))?;
        Ok(())
    }

    async fn find_template(
        &self,
        id: Uuid,
    ) -> Result<Option<SiteTemplate>, SiteCloneTemplateError> {
        let row = sqlx::query(
            "SELECT id, name, source_site_id, artifact_path, signature, signature_valid,
                    warnings, pii_policy, created_at
             FROM site_templates WHERE id = ?1",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("find template: {e}")))?;
        let Some(row) = row else { return Ok(None) };
        let id: String = row.get("id");
        let name: String = row.get("name");
        let source_site_id: String = row.get("source_site_id");
        let artifact_path: String = row.get("artifact_path");
        let signature: String = row.get("signature");
        let signature_valid: i64 = row.get("signature_valid");
        let warnings: String = row.get("warnings");
        let pii_policy: String = row.get("pii_policy");
        let created_at: String = row.get("created_at");
        let id = Uuid::parse_str(&id)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("id: {e}")))?;
        let source_site_id = Uuid::parse_str(&source_site_id)
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("source: {e}")))?;
        let mut template = SiteTemplate::new(
            id,
            name,
            source_site_id,
            artifact_path,
            signature,
            serde_json::from_str(&warnings)
                .map_err(|e| SiteCloneTemplateError::Persistence(format!("warnings: {e}")))?,
            parse_ts(&created_at),
        )
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("template: {e}")))?;
        if signature_valid == 0 {
            template.mark_signature_invalid();
        }
        // pii_policy is fixed at `None` for templates; we don't re-apply the stored value.
        let _ = pii_policy;
        Ok(Some(template))
    }

    async fn list_templates(&self) -> Result<Vec<SiteTemplate>, SiteCloneTemplateError> {
        let rows = sqlx::query("SELECT id FROM site_templates ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("list templates: {e}")))?;
        let mut out = Vec::new();
        for row in rows {
            let id: String = row.get("id");
            let id = Uuid::parse_str(&id)
                .map_err(|e| SiteCloneTemplateError::Persistence(format!("id: {e}")))?;
            if let Some(t) = self.find_template(id).await? {
                out.push(t);
            }
        }
        Ok(out)
    }

    async fn mark_template_signature_invalid(
        &self,
        id: Uuid,
    ) -> Result<(), SiteCloneTemplateError> {
        sqlx::query("UPDATE site_templates SET signature_valid = 0 WHERE id = ?1")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| SiteCloneTemplateError::Persistence(format!("mark sig: {e}")))?;
        Ok(())
    }

    async fn insert_anonymisation_token(
        &self,
        token: &AnonymisationToken,
    ) -> Result<(), SiteCloneTemplateError> {
        sqlx::query(
            "INSERT INTO anonymisation_tokens
             (id, run_id, cipher_text, original_hash, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .bind(token.id().to_string())
        .bind(token.run_id().to_string())
        .bind(token.cipher_text())
        .bind(token.original_hash())
        .bind(token.created_at().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| SiteCloneTemplateError::Persistence(format!("insert token: {e}")))?;
        Ok(())
    }
}
