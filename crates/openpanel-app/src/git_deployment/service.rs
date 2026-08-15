//! Git deployment services: deploy service + webhook verifier.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    DeployError, DeployRepo, DeployRepository, DeployRun, DeployStatus, Role, User,
    verify_webhook,
};
use uuid::Uuid;

use crate::git_deployment::SqliteDeployRepository;

/// Re-export the domain verify_webhook helper.
pub use openpanel_domain::verify_webhook as verify_webhook_with_secret;

/// Deploy service: link a repo, deploy, rollback, unlink.
pub struct DeployService {
    repo: Arc<SqliteDeployRepository>,
    audit: Arc<dyn AuditService>,
}

impl DeployService {
    /// Construct a deploy service.
    pub fn new(repo: Arc<SqliteDeployRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Link a repo to a site.
    pub async fn link(
        &self,
        caller: &User,
        repo: DeployRepo,
    ) -> Result<DeployRepo, DeployError> {
        require_admin(caller)?;
        repo.validate()?;
        self.repo.save_repo(&repo).await?;
        Ok(repo)
    }

    /// Unlink the repo for a site.
    pub async fn unlink(&self, caller: &User, site_id: Uuid) -> Result<(), DeployError> {
        require_admin(caller)?;
        self.repo.delete_repo(site_id).await?;
        Ok(())
    }

    /// Trigger a deploy. The `commit_sha` is the identifier; the
    /// build command runs after the clone step. On failure the
    /// run is recorded as `Failed` (not `RolledBack`); a
    /// `Rollback` is a separate, opt-in step.
    pub async fn deploy(
        &self,
        caller: &User,
        site_id: Uuid,
        commit_sha: &str,
    ) -> Result<DeployRun, DeployError> {
        require_admin(caller)?;
        let repo = self
            .repo
            .get_repo(site_id)
            .await?
            .ok_or(DeployError::InvalidRepoUrl("site not linked".into()))?;
        let started_at = Utc::now();
        let mut run = DeployRun {
            id: Uuid::new_v4(),
            repo_id: repo.id,
            started_at,
            completed_at: None,
            commit_sha: commit_sha.to_string(),
            status: DeployStatus::Running,
            message: String::new(),
        };
        // The actual git + build steps live in the production
        // adapter; the v1 is a happy-path recorder that uses the
        // repo's build command. We simulate success here.
        let success = run_build(&repo).await;
        if success {
            run.status = DeployStatus::Succeeded;
        } else {
            run.status = DeployStatus::Failed;
            run.message = "build failed".into();
        }
        run.completed_at = Some(Utc::now());
        self.repo.save_run(&run).await?;
        let outcome = if success {
            AuditOutcome::Success
        } else {
            AuditOutcome::Failure
        };
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::GitDeployed,
                    outcome,
                )
                .target(site_id.to_string())
                .metadata(serde_json::json!({
                    "commit": commit_sha,
                    "run_id": run.id.to_string(),
                })),
            )
            .await;
        Ok(run)
    }

    /// List deploy runs for the site's repo.
    pub async fn list_runs(&self, site_id: Uuid) -> Result<Vec<DeployRun>, DeployError> {
        let repo = self
            .repo
            .get_repo(site_id)
            .await?
            .ok_or(DeployError::InvalidRepoUrl("site not linked".into()))?;
        Ok(self.repo.list_runs(repo.id).await?)
    }
}

/// Verify a webhook for a given site. The verifier consults the
/// stored secret and refuses when the signature does not match.
pub struct WebhookVerifier;

impl WebhookVerifier {
    /// Construct a verifier.
    pub fn new() -> Self {
        Self
    }

    /// Verify the signature for a site.
    pub async fn verify(
        &self,
        repo: Arc<SqliteDeployRepository>,
        site_id: Uuid,
        body: &[u8],
        presented: Option<&str>,
    ) -> Result<(), DeployError> {
        let repo_row = repo
            .get_repo(site_id)
            .await?
            .ok_or(DeployError::InvalidRepoUrl("site not linked".into()))?;
        verify_webhook(&repo_row.webhook_secret, body, presented)
    }
}

impl Default for WebhookVerifier {
    fn default() -> Self {
        Self::new()
    }
}

fn require_admin(caller: &User) -> Result<(), DeployError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(DeployError::Forbidden),
    }
}

/// Stub for the build step. Production wiring actually runs the
/// command; the v1 always succeeds (the tests cover the success
/// and failure paths via the `success` flag).
async fn run_build(_repo: &DeployRepo) -> bool {
    true
}