//! Agent application service: registration, heartbeat, and recipe
//! dispatch.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AgentError, AgentId, AgentRegistration, AgentRepository, FleetToken, FleetTokenScope,
    RecipeManifest, is_manifest_signature_valid,
};
use uuid::Uuid;

use crate::agent::repo::SqliteAgentRepository;

const MANIFEST_SIGNATURE_PREFIX: &str = "openpanel-v1:";

/// Agent application service.
#[derive(Clone)]
pub struct AgentService {
    repo: Arc<dyn AgentRepository>,
    audit: Arc<dyn AuditService>,
}

impl AgentService {
    /// Build a service over the given repository.
    pub fn new(repo: Arc<dyn AgentRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Build a service backed by the SQLite adapter.
    pub fn with_sqlite(pool: sqlx::Pool<sqlx::Sqlite>, audit: Arc<dyn AuditService>) -> Self {
        Self::new(Arc::new(SqliteAgentRepository::new(pool)), audit)
    }

    /// Register a new agent.
    pub async fn register(
        &self,
        host_fingerprint: &str,
        hostname: &str,
        cert_fingerprint: &str,
        owner_id: Uuid,
        actor: &str,
    ) -> Result<AgentRegistration, AgentError> {
        if self.repo.find_agent_by_cert(cert_fingerprint).await?.is_some() {
            return Err(AgentError::InvalidManifest(
                "agent with that cert fingerprint already registered".to_string(),
            ));
        }
        let agent = AgentRegistration::new(
            AgentId::new(),
            host_fingerprint.to_string(),
            hostname.to_string(),
            cert_fingerprint.to_string(),
            owner_id,
            Utc::now(),
        );
        self.repo.insert_agent(&agent).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::AgentRegistered, AuditOutcome::Success)
                    .target(agent.id().as_uuid().to_string())
                    .metadata(serde_json::json!({
                        "hostname": agent.hostname(),
                        "host_fingerprint": agent.host_fingerprint(),
                    })),
            )
            .await
            .ok();
        Ok(agent)
    }

    /// Mark an agent online with a fresh heartbeat.
    pub async fn heartbeat(
        &self,
        agent_id: AgentId,
    ) -> Result<(), AgentError> {
        let mut agent = self
            .repo
            .find_agent(agent_id)
            .await?
            .ok_or(AgentError::AgentNotFound)?;
        if matches!(agent.status(), openpanel_domain::AgentStatus::Revoked) {
            return Err(AgentError::Revoked);
        }
        agent.mark_online(Utc::now());
        self.repo.update_agent(&agent).await?;
        Ok(())
    }

    /// Revoke an agent.
    pub async fn revoke(
        &self,
        agent_id: AgentId,
        actor: &str,
    ) -> Result<AgentRegistration, AgentError> {
        let mut agent = self
            .repo
            .find_agent(agent_id)
            .await?
            .ok_or(AgentError::AgentNotFound)?;
        agent.revoke();
        self.repo.update_agent(&agent).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::AgentRevoked, AuditOutcome::Success)
                    .target(agent.id().as_uuid().to_string()),
            )
            .await
            .ok();
        Ok(agent)
    }

    /// List agents.
    pub async fn list(&self) -> Result<Vec<AgentRegistration>, AgentError> {
        self.repo.list_agents().await
    }

    /// Find an agent.
    pub async fn find(&self, id: AgentId) -> Result<Option<AgentRegistration>, AgentError> {
        self.repo.find_agent(id).await
    }

    /// Find an agent by its mTLS client cert fingerprint.
    pub async fn find_by_cert(
        &self,
        cert_fingerprint: &str,
    ) -> Result<Option<AgentRegistration>, AgentError> {
        self.repo.find_agent_by_cert(cert_fingerprint).await
    }

    /// Issue a FleetToken for an agent.
    pub async fn issue_token(
        &self,
        agent_id: AgentId,
        scope: FleetTokenScope,
        token_hash: &str,
        ttl_seconds: i64,
        actor: &str,
    ) -> Result<FleetToken, AgentError> {
        let agent = self
            .repo
            .find_agent(agent_id)
            .await?
            .ok_or(AgentError::AgentNotFound)?;
        if !agent.is_usable() {
            return Err(AgentError::Revoked);
        }
        let now = Utc::now();
        let token = FleetToken::new(
            Uuid::new_v4(),
            agent_id,
            scope,
            token_hash.to_string(),
            now,
            now + chrono::Duration::seconds(ttl_seconds),
        );
        self.repo.insert_token(&token).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::FleetTokenIssued, AuditOutcome::Success)
                    .target(agent_id.as_uuid().to_string())
                    .metadata(serde_json::json!({"scope": format!("{:?}", scope)})),
            )
            .await
            .ok();
        Ok(token)
    }

    /// Revoke a token.
    pub async fn revoke_token(
        &self,
        token_id: Uuid,
        actor: &str,
    ) -> Result<(), AgentError> {
        let mut token = self
            .repo
            .find_token(token_id)
            .await?
            .ok_or(AgentError::TokenNotFound)?;
        token.revoke();
        self.repo.update_token(&token).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::FleetTokenRevoked, AuditOutcome::Success)
                    .target(token_id.to_string()),
            )
            .await
            .ok();
        Ok(())
    }

    /// Resolve a token by hash, returning `None` for missing or
    /// expired tokens.
    pub async fn resolve_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<FleetToken>, AgentError> {
        let token = self.repo.find_token_by_hash(token_hash).await?;
        Ok(token.filter(|t| t.is_usable_at(Utc::now())))
    }

    /// Persist a recipe manifest after validating the signature.
    pub async fn store_manifest(
        &self,
        manifest: RecipeManifest,
        actor: &str,
    ) -> Result<RecipeManifest, AgentError> {
        if !is_manifest_signature_valid(&manifest, MANIFEST_SIGNATURE_PREFIX) {
            return Err(AgentError::SignatureInvalid);
        }
        if manifest.is_expired_at(Utc::now()) {
            return Err(AgentError::ManifestExpired);
        }
        self.repo.insert_manifest(&manifest).await?;
        self.audit
            .record(
                AuditEvent::new(actor, AuditAction::RecipeManifestStored, AuditOutcome::Success)
                    .target(manifest.id().to_string())
                    .metadata(serde_json::json!({"name": manifest.name()})),
            )
            .await
            .ok();
        Ok(manifest)
    }

    /// Find a stored manifest.
    pub async fn find_manifest(&self, id: Uuid) -> Result<Option<RecipeManifest>, AgentError> {
        self.repo.find_manifest(id).await
    }

    /// Validate a dispatched manifest against the agent's allowlist
    /// and the recipe's runner set.
    pub async fn validate_dispatch(
        &self,
        agent_id: AgentId,
        manifest_id: Uuid,
        runner: &str,
    ) -> Result<RecipeManifest, AgentError> {
        let agent = self
            .repo
            .find_agent(agent_id)
            .await?
            .ok_or(AgentError::AgentNotFound)?;
        if !agent.is_usable() {
            return Err(AgentError::Revoked);
        }
        let manifest = self
            .repo
            .find_manifest(manifest_id)
            .await?
            .ok_or(AgentError::InvalidManifest("manifest not found".to_string()))?;
        if manifest.is_expired_at(Utc::now()) {
            return Err(AgentError::ManifestExpired);
        }
        if !manifest.allows_runner(runner) {
            return Err(AgentError::RunnerNotAllowed(runner.to_string()));
        }
        Ok(manifest)
    }
}
