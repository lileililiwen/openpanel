//! SQLite-backed adapter for the agent bounded context.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_domain::{
    AgentError, AgentId, AgentRegistration, AgentRepository, AgentStatus, FleetToken,
    FleetTokenScope, RecipeManifest, agent::RecipeAction,
};
use sqlx::{Pool, Sqlite};
use uuid::Uuid;

/// SQLite-backed agent repository.
#[derive(Clone)]
pub struct SqliteAgentRepository {
    pool: Pool<Sqlite>,
}

impl SqliteAgentRepository {
    /// Build a repo over the given pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AgentRepository for SqliteAgentRepository {
    async fn insert_agent(&self, agent: &AgentRegistration) -> Result<(), AgentError> {
        sqlx::query("INSERT INTO agents (id, host_fingerprint, hostname, status, cert_fingerprint, last_heartbeat_at, registered_at, owner_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(agent.id().as_uuid().to_string())
            .bind(agent.host_fingerprint())
            .bind(agent.hostname())
            .bind(agent_status_str(agent.status()))
            .bind(agent.cert_fingerprint())
            .bind(agent.last_heartbeat_at().map(|d| d.to_rfc3339()))
            .bind(agent.registered_at().to_rfc3339())
            .bind(agent.owner_id().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_agent(&self, id: AgentId) -> Result<Option<AgentRegistration>, AgentError> {
        let row: Option<AgentRow> = sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_fingerprint, hostname, status, cert_fingerprint, last_heartbeat_at, registered_at, owner_id FROM agents WHERE id = ?",
        )
        .bind(id.as_uuid().to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        row.map(AgentRow::into_agent).transpose()
    }

    async fn find_agent_by_cert(
        &self,
        cert_fingerprint: &str,
    ) -> Result<Option<AgentRegistration>, AgentError> {
        let row: Option<AgentRow> = sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_fingerprint, hostname, status, cert_fingerprint, last_heartbeat_at, registered_at, owner_id FROM agents WHERE cert_fingerprint = ?",
        )
        .bind(cert_fingerprint)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        row.map(AgentRow::into_agent).transpose()
    }

    async fn update_agent(&self, agent: &AgentRegistration) -> Result<(), AgentError> {
        sqlx::query(
            "UPDATE agents SET status = ?, last_heartbeat_at = ?, hostname = ? WHERE id = ?",
        )
        .bind(agent_status_str(agent.status()))
        .bind(agent.last_heartbeat_at().map(|d| d.to_rfc3339()))
        .bind(agent.hostname())
        .bind(agent.id().as_uuid().to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn list_agents(&self) -> Result<Vec<AgentRegistration>, AgentError> {
        let rows: Vec<AgentRow> = sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_fingerprint, hostname, status, cert_fingerprint, last_heartbeat_at, registered_at, owner_id FROM agents ORDER BY registered_at",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        rows.into_iter().map(AgentRow::into_agent).collect()
    }

    async fn insert_token(&self, token: &FleetToken) -> Result<(), AgentError> {
        sqlx::query("INSERT INTO fleet_tokens (id, agent_id, scope, token_hash, issued_at, expires_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(token.id().to_string())
            .bind(token.agent_id().as_uuid().to_string())
            .bind(token_scope_str(token.scope()))
            .bind(token.token_hash())
            .bind(token.issued_at().to_rfc3339())
            .bind(token.expires_at().to_rfc3339())
            .bind(if token.is_revoked() { 1 } else { 0 })
            .execute(&self.pool)
            .await
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_token(&self, id: Uuid) -> Result<Option<FleetToken>, AgentError> {
        let row: Option<TokenRow> = sqlx::query_as::<_, TokenRow>(
            "SELECT id, agent_id, scope, token_hash, issued_at, expires_at, revoked FROM fleet_tokens WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        row.map(TokenRow::into_token).transpose()
    }

    async fn update_token(&self, token: &FleetToken) -> Result<(), AgentError> {
        sqlx::query("UPDATE fleet_tokens SET revoked = ? WHERE id = ?")
            .bind(if token.is_revoked() { 1 } else { 0 })
            .bind(token.id().to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_token_by_hash(&self, token_hash: &str) -> Result<Option<FleetToken>, AgentError> {
        let row: Option<TokenRow> = sqlx::query_as::<_, TokenRow>(
            "SELECT id, agent_id, scope, token_hash, issued_at, expires_at, revoked FROM fleet_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        row.map(TokenRow::into_token).transpose()
    }

    async fn insert_manifest(&self, manifest: &RecipeManifest) -> Result<(), AgentError> {
        let allowed_runners: Vec<String> = manifest.allowed_runners().iter().cloned().collect();
        let actions_json = serde_json::to_string(manifest.actions())
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        let rollback_actions_json = serde_json::to_string(manifest.rollback_actions())
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        let allowed_runners_json = serde_json::to_string(&allowed_runners)
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        sqlx::query("INSERT INTO recipe_manifests (id, name, actions_json, rollback_actions_json, allowed_runners_json, signature, signed_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(manifest.id().to_string())
            .bind(manifest.name())
            .bind(actions_json)
            .bind(rollback_actions_json)
            .bind(allowed_runners_json)
            .bind(manifest.signature())
            .bind(manifest.signed_at().to_rfc3339())
            .bind(manifest.expires_at().to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        Ok(())
    }

    async fn find_manifest(&self, id: Uuid) -> Result<Option<RecipeManifest>, AgentError> {
        let row: Option<ManifestRow> = sqlx::query_as::<_, ManifestRow>(
            "SELECT id, name, actions_json, rollback_actions_json, allowed_runners_json, signature, signed_at, expires_at FROM recipe_manifests WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AgentError::Persistence(e.to_string()))?;
        row.map(ManifestRow::into_manifest).transpose()
    }
}

fn agent_status_str(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Pending => "pending",
        AgentStatus::Online => "online",
        AgentStatus::Offline => "offline",
        AgentStatus::Revoked => "revoked",
    }
}

fn parse_agent_status(s: &str) -> Result<AgentStatus, AgentError> {
    match s {
        "pending" => Ok(AgentStatus::Pending),
        "online" => Ok(AgentStatus::Online),
        "offline" => Ok(AgentStatus::Offline),
        "revoked" => Ok(AgentStatus::Revoked),
        other => Err(AgentError::Persistence(format!(
            "unknown agent status `{other}`"
        ))),
    }
}

fn token_scope_str(scope: FleetTokenScope) -> &'static str {
    match scope {
        FleetTokenScope::Read => "read",
        FleetTokenScope::Execute => "execute",
        FleetTokenScope::Admin => "admin",
    }
}

fn parse_token_scope(s: &str) -> Result<FleetTokenScope, AgentError> {
    match s {
        "read" => Ok(FleetTokenScope::Read),
        "execute" => Ok(FleetTokenScope::Execute),
        "admin" => Ok(FleetTokenScope::Admin),
        other => Err(AgentError::Persistence(format!(
            "unknown token scope `{other}`"
        ))),
    }
}

#[derive(sqlx::FromRow)]
struct AgentRow {
    id: String,
    host_fingerprint: String,
    hostname: String,
    status: String,
    cert_fingerprint: String,
    last_heartbeat_at: Option<String>,
    registered_at: String,
    owner_id: String,
}

impl AgentRow {
    fn into_agent(self) -> Result<AgentRegistration, AgentError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| AgentError::Persistence(format!("bad id: {e}")))?;
        let owner_id = Uuid::parse_str(&self.owner_id)
            .map_err(|e| AgentError::Persistence(format!("bad owner_id: {e}")))?;
        let status = parse_agent_status(&self.status)?;
        let registered_at = parse_dt(&self.registered_at)?;
        let last_heartbeat_at = self
            .last_heartbeat_at
            .as_deref()
            .map(parse_dt)
            .transpose()?;
        Ok(AgentRegistration::restore(
            AgentId(id),
            self.host_fingerprint,
            self.hostname,
            status,
            self.cert_fingerprint,
            last_heartbeat_at,
            registered_at,
            owner_id,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct TokenRow {
    id: String,
    agent_id: String,
    scope: String,
    token_hash: String,
    issued_at: String,
    expires_at: String,
    revoked: i64,
}

impl TokenRow {
    fn into_token(self) -> Result<FleetToken, AgentError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| AgentError::Persistence(format!("bad id: {e}")))?;
        let agent_id = Uuid::parse_str(&self.agent_id)
            .map_err(|e| AgentError::Persistence(format!("bad agent_id: {e}")))?;
        let scope = parse_token_scope(&self.scope)?;
        let issued_at = parse_dt(&self.issued_at)?;
        let expires_at = parse_dt(&self.expires_at)?;
        Ok(FleetToken::restore(
            id,
            AgentId(agent_id),
            scope,
            self.token_hash,
            issued_at,
            expires_at,
            self.revoked != 0,
        ))
    }
}

#[derive(sqlx::FromRow)]
struct ManifestRow {
    id: String,
    name: String,
    actions_json: String,
    rollback_actions_json: String,
    allowed_runners_json: String,
    signature: String,
    signed_at: String,
    expires_at: String,
}

impl ManifestRow {
    fn into_manifest(self) -> Result<RecipeManifest, AgentError> {
        let id = Uuid::parse_str(&self.id)
            .map_err(|e| AgentError::Persistence(format!("bad id: {e}")))?;
        let actions: Vec<RecipeAction> = serde_json::from_str(&self.actions_json)
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        let rollback_actions: Vec<RecipeAction> = serde_json::from_str(&self.rollback_actions_json)
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        let allowed_runners: Vec<String> = serde_json::from_str(&self.allowed_runners_json)
            .map_err(|e| AgentError::Persistence(e.to_string()))?;
        let signed_at = parse_dt(&self.signed_at)?;
        let expires_at = parse_dt(&self.expires_at)?;
        Ok(RecipeManifest::restore(
            id,
            self.name,
            actions,
            rollback_actions,
            allowed_runners.into_iter().collect(),
            self.signature,
            signed_at,
            expires_at,
        ))
    }
}

fn parse_dt(s: &str) -> Result<DateTime<Utc>, AgentError> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| AgentError::Persistence(format!("bad timestamp `{s}`: {e}")))
}
