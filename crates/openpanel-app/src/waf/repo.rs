//! SQLite WAF repository adapter.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    RepoError,
    waf::{RuleAction, RuleSet, WafHit, WafRepository},
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed WAF policy and hit repository.
pub struct SqliteWafRepository {
    pool: Pool<Sqlite>,
}

impl SqliteWafRepository {
    /// Construct the adapter over a SQLite pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl WafRepository for SqliteWafRepository {
    async fn get(&self, site_id: Uuid) -> Result<Option<RuleSet>, RepoError> {
        let document = sqlx::query_scalar::<_, String>(
            "SELECT document_json FROM waf_rule_sets WHERE site_id = ?",
        )
        .bind(site_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(repo_error)?;
        document
            .map(|json| {
                serde_json::from_str::<RuleSet>(&json)
                    .map_err(repo_error)
                    .and_then(|set| set.validated().map_err(repo_error))
            })
            .transpose()
    }

    async fn put(&self, ruleset: &RuleSet) -> Result<(), RepoError> {
        let json = serde_json::to_string(ruleset).map_err(repo_error)?;
        sqlx::query(
            "INSERT INTO waf_rule_sets (site_id, document_json, updated_at) VALUES (?, ?, ?) \
             ON CONFLICT(site_id) DO UPDATE SET document_json = excluded.document_json, \
             updated_at = excluded.updated_at",
        )
        .bind(ruleset.site_id().to_string())
        .bind(json)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn record_hit(&self, hit: &WafHit) -> Result<(), RepoError> {
        let count = i64::try_from(hit.count).map_err(repo_error)?;
        sqlx::query(
            "INSERT INTO waf_hits \
             (site_id, rule_id, kind, action, count, last_triggered_at) VALUES (?, ?, ?, ?, ?, ?) \
             ON CONFLICT(site_id, rule_id) DO UPDATE SET \
             kind = excluded.kind, action = excluded.action, \
             count = waf_hits.count + excluded.count, \
             last_triggered_at = excluded.last_triggered_at",
        )
        .bind(hit.site_id.to_string())
        .bind(hit.rule_id.to_string())
        .bind(&hit.kind)
        .bind(action_str(hit.action))
        .bind(count)
        .bind(hit.last_triggered_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn hits(&self, site_id: Uuid) -> Result<Vec<WafHit>, RepoError> {
        let rows = sqlx::query_as::<_, HitRow>(
            "SELECT site_id, rule_id, kind, action, count, last_triggered_at \
             FROM waf_hits WHERE site_id = ? ORDER BY last_triggered_at DESC, rule_id",
        )
        .bind(site_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(repo_error)?;
        rows.into_iter().map(HitRow::into_hit).collect()
    }
}

#[derive(sqlx::FromRow)]
struct HitRow {
    site_id: String,
    rule_id: String,
    kind: String,
    action: String,
    count: i64,
    last_triggered_at: String,
}

impl HitRow {
    fn into_hit(self) -> Result<WafHit, RepoError> {
        Ok(WafHit {
            site_id: Uuid::parse_str(&self.site_id).map_err(repo_error)?,
            rule_id: Uuid::parse_str(&self.rule_id).map_err(repo_error)?,
            kind: self.kind,
            action: parse_action(&self.action)?,
            count: u64::try_from(self.count).map_err(repo_error)?,
            last_triggered_at: DateTime::parse_from_rfc3339(&self.last_triggered_at)
                .map_err(repo_error)?
                .with_timezone(&Utc),
        })
    }
}

fn action_str(action: RuleAction) -> &'static str {
    match action {
        RuleAction::Allow => "allow",
        RuleAction::Challenge => "challenge",
        RuleAction::Deny => "deny",
    }
}

fn parse_action(value: &str) -> Result<RuleAction, RepoError> {
    match value {
        "allow" => Ok(RuleAction::Allow),
        "challenge" => Ok(RuleAction::Challenge),
        "deny" => Ok(RuleAction::Deny),
        _ => Err(RepoError::new("invalid persisted WAF action")),
    }
}

fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}
