//! Compliance services: CIS hardening wizard, audit retention
//! service, and GDPR exporter with secret redaction.

use std::sync::Arc;

use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    AuditRetentionPolicy, ComplianceError, ComplianceRepository, GdprApiTokenRecord,
    GdprDatabaseRecord, GdprExport, GdprExportPayload, GdprMailRecord, GdprSiteRecord, REDACTED,
    Role, User,
};
use uuid::Uuid;

use crate::compliance::SqliteComplianceRepository;

/// Port that applies and rolls back a single CIS rule on the host.
/// Production implementations are responsible for actually mutating
/// the host configuration; tests use the in-memory fake.
pub trait RuleExecutor: Send + Sync + 'static {
    /// Apply the rule and return the pre/post images.
    fn apply(&self, rule_id: &str, title: &str) -> Result<RuleImages, ComplianceError>;
    /// Roll back a previously applied rule to its pre-image.
    fn rollback(&self, rule_id: &str, pre_image: &serde_json::Value)
    -> Result<(), ComplianceError>;
}

/// Pre/post images for a single rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleImages {
    /// Snapshot of the prior host state.
    pub pre: serde_json::Value,
    /// Snapshot of the post-apply state.
    pub post: serde_json::Value,
}

/// Default executor that records every apply/rollback in memory.
/// Production wiring points this at a real shell-out adapter; the
/// in-memory executor lets tests exercise the contract end-to-end.
#[derive(Default)]
pub struct InMemoryRuleExecutor {
    /// Last-seen image per rule, captured for replay.
    state: std::sync::Mutex<std::collections::HashMap<String, serde_json::Value>>,
}

impl InMemoryRuleExecutor {
    /// Construct an empty in-memory executor.
    pub fn new() -> Self {
        Self::default()
    }
}

impl RuleExecutor for InMemoryRuleExecutor {
    fn apply(&self, rule_id: &str, title: &str) -> Result<RuleImages, ComplianceError> {
        #[allow(clippy::expect_used)]
        // The mutex is only poisoned if a previous holder panicked; this executor has no panic paths.
        let mut state = self.state.lock().expect("state");
        let pre = state
            .entry(rule_id.to_string())
            .or_insert_with(|| serde_json::json!({"title": title, "applied": false}));
        let post = serde_json::json!({"title": title, "applied": true});
        let pre_copy = pre.clone();
        *pre = post.clone();
        Ok(RuleImages {
            pre: pre_copy,
            post,
        })
    }

    fn rollback(
        &self,
        rule_id: &str,
        pre_image: &serde_json::Value,
    ) -> Result<(), ComplianceError> {
        #[allow(clippy::expect_used)]
        // The mutex is only poisoned if a previous holder panicked; this executor has no panic paths.
        let mut state = self.state.lock().expect("state");
        state.insert(rule_id.to_string(), pre_image.clone());
        Ok(())
    }
}

/// Outcome of a single harden wizard invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardeningOutcome {
    /// Stable id of the persisted run.
    pub run_id: Uuid,
    /// True if at least one rule failed.
    pub has_failures: bool,
    /// Per-rule results.
    pub rules: Vec<openpanel_domain::HardeningRule>,
}

/// CIS hardening wizard. Applies every rule in a profile, records
/// pre/post images, and emits a `HardeningApplied` audit event.
pub struct HardeningWizard {
    repo: Arc<SqliteComplianceRepository>,
    audit: Arc<dyn AuditService>,
    executor: Arc<dyn RuleExecutor>,
}

impl HardeningWizard {
    /// Construct a wizard with the default in-memory executor.
    pub fn new(repo: Arc<SqliteComplianceRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            repo,
            audit,
            executor: Arc::new(InMemoryRuleExecutor::new()),
        }
    }

    /// Override the rule executor (production wiring).
    pub fn with_executor(mut self, executor: Arc<dyn RuleExecutor>) -> Self {
        self.executor = executor;
        self
    }

    /// Apply a profile. The caller MUST be Admin or Owner.
    pub async fn harden(
        &self,
        caller: &User,
        profile: &str,
    ) -> Result<HardeningOutcome, ComplianceError> {
        require_admin(caller)?;
        let rules = rules_for_profile(profile);
        let mut run = openpanel_domain::HardeningRun::new(profile, caller.id());
        for rule in rules {
            let id = rule.id.clone();
            let title = rule.title.clone();
            match self.executor.apply(&id, &title) {
                Ok(images) => {
                    run.record(openpanel_domain::HardeningRule::applied(
                        id,
                        title,
                        images.pre,
                        images.post,
                    ));
                }
                Err(error) => {
                    run.record(openpanel_domain::HardeningRule::failed(
                        id,
                        title,
                        error.to_string(),
                    ));
                }
            }
        }
        run.finish();
        self.repo.save_hardening_run(&run).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::HardeningApplied,
                    AuditOutcome::Success,
                )
                .target(run.id.to_string())
                .metadata(serde_json::json!({
                    "profile": profile,
                    "rule_count": run.rules.len(),
                    "has_failures": run.has_failures,
                })),
            )
            .await;
        Ok(HardeningOutcome {
            run_id: run.id,
            has_failures: run.has_failures,
            rules: run.rules,
        })
    }

    /// Roll back a hardening run to its pre-image for every
    /// `Applied` rule. Caller MUST be Admin/Owner.
    pub async fn rollback(
        &self,
        caller: &User,
        run_id: Uuid,
    ) -> Result<RollbackOutcome, ComplianceError> {
        require_admin(caller)?;
        let run = self
            .repo
            .get_hardening_run(run_id)
            .await?
            .ok_or(ComplianceError::HardeningRunNotFound(run_id))?;
        let mut reverted = Vec::new();
        for rule in run.rollback_targets() {
            self.executor.rollback(&rule.id, &rule.pre_image)?;
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::HardeningReverted,
                        AuditOutcome::Success,
                    )
                    .target(rule.id.clone())
                    .metadata(serde_json::json!({
                        "run_id": run.id.to_string(),
                    })),
                )
                .await;
            reverted.push(rule.id.clone());
        }
        Ok(RollbackOutcome { run_id, reverted })
    }
}

/// Outcome of a rollback request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackOutcome {
    /// Run id that was rolled back.
    pub run_id: Uuid,
    /// Per-rule ids that were reverted.
    pub reverted: Vec<String>,
}

/// Audit retention service: persists the policy and runs the purge.
pub struct AuditRetentionService {
    repo: Arc<SqliteComplianceRepository>,
    audit: Arc<dyn AuditService>,
}

impl AuditRetentionService {
    /// Construct a retention service.
    pub fn new(repo: Arc<SqliteComplianceRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Get the current retention policy, falling back to the
    /// caller-supplied default when none is stored.
    pub async fn get(&self, caller: &User) -> Result<AuditRetentionPolicy, ComplianceError> {
        require_admin(caller)?;
        Ok(self
            .repo
            .get_retention_policy()
            .await?
            .unwrap_or_else(|| AuditRetentionPolicy::default(caller.id())))
    }

    /// Persist a new retention policy.
    pub async fn set(
        &self,
        caller: &User,
        ttl_days: u32,
        export_before_purge: bool,
    ) -> Result<AuditRetentionPolicy, ComplianceError> {
        require_admin(caller)?;
        let policy = AuditRetentionPolicy {
            ttl_days,
            export_before_purge,
            updated_at: chrono::Utc::now(),
            updated_by: caller.id(),
        };
        policy.validate()?;
        self.repo.save_retention_policy(&policy).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AuditRetentionChanged,
                    AuditOutcome::Success,
                )
                .target("audit-retention".to_string())
                .metadata(serde_json::json!({
                    "ttl_days": ttl_days,
                    "export_before_purge": export_before_purge,
                })),
            )
            .await;
        Ok(policy)
    }

    /// Run the purge against the supplied audit log port. Returns
    /// the number of records that were removed.
    pub async fn run_purge(
        &self,
        caller: &User,
        log: &dyn AuditPurge,
    ) -> Result<u64, ComplianceError> {
        require_admin(caller)?;
        let policy = self.get(caller).await?;
        let cutoff = chrono::Utc::now() - chrono::Duration::days(policy.ttl_days as i64);
        let count = log.purge_before(cutoff).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::AuditPurged,
                    AuditOutcome::Success,
                )
                .target("audit-log".to_string())
                .metadata(serde_json::json!({
                    "count": count,
                    "cutoff": cutoff.to_rfc3339(),
                })),
            )
            .await;
        Ok(count)
    }
}

/// Port the retention service uses to actually delete audit records.
#[async_trait::async_trait]
pub trait AuditPurge: Send + Sync {
    /// Delete every audit record with `ts < cutoff` and return the
    /// number removed.
    async fn purge_before(
        &self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, ComplianceError>;
}

/// GDPR data exporter. Aggregates the user's PII across sites, mail,
/// and databases, with secrets redacted to a sentinel placeholder.
pub struct GdprExporter {
    repo: Arc<SqliteComplianceRepository>,
    audit: Arc<dyn AuditService>,
    sources: Arc<dyn GdprSources>,
}

impl GdprExporter {
    /// Construct an exporter with the default (empty) source port.
    pub fn new(repo: Arc<SqliteComplianceRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self {
            repo,
            audit,
            sources: Arc::new(EmptyGdprSources),
        }
    }

    /// Override the source port (production wiring).
    pub fn with_sources(mut self, sources: Arc<dyn GdprSources>) -> Self {
        self.sources = sources;
        self
    }

    /// Build a fresh export for the given user.
    pub async fn export(
        &self,
        caller: &User,
        user_id: Uuid,
    ) -> Result<GdprExport, ComplianceError> {
        require_admin(caller)?;
        let sites = self
            .sources
            .sites_for(user_id)
            .await?
            .into_iter()
            .map(|s| GdprSiteRecord {
                id: s.id,
                domain: s.domain,
                aliases: s.aliases,
                owner_id: s.owner_id,
            })
            .collect();
        let mail = self
            .sources
            .mail_for(user_id)
            .await?
            .into_iter()
            .map(|m| GdprMailRecord {
                address: m.address,
                display_name: m.display_name,
                quota_bytes: m.quota_bytes,
            })
            .collect();
        let databases = self
            .sources
            .databases_for(user_id)
            .await?
            .into_iter()
            .map(|d| GdprDatabaseRecord {
                id: d.id,
                name: d.name,
                owner_id: d.owner_id,
                password: REDACTED.to_string(),
            })
            .collect();
        let api_tokens = self
            .sources
            .api_tokens_for(user_id)
            .await?
            .into_iter()
            .map(|t| GdprApiTokenRecord {
                id: t.id,
                name: t.name,
                scopes: t.scopes,
                token: REDACTED.to_string(),
            })
            .collect();
        let export = GdprExport {
            id: Uuid::new_v4(),
            user_id,
            payload: GdprExportPayload {
                sites,
                mail,
                databases,
                api_tokens,
            },
            generated_at: chrono::Utc::now(),
        };
        self.repo.save_gdpr_export(&export).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::GdprExportRequested,
                    AuditOutcome::Success,
                )
                .target(user_id.to_string())
                .metadata(serde_json::json!({
                    "sites": export.payload.sites.len(),
                    "mail": export.payload.mail.len(),
                    "databases": export.payload.databases.len(),
                    "api_tokens": export.payload.api_tokens.len(),
                })),
            )
            .await;
        Ok(export)
    }
}

/// Site record supplied to the GDPR exporter.
#[derive(Debug, Clone)]
pub struct GdprSourceSite {
    /// Site record id.
    pub id: Uuid,
    /// Primary domain name.
    pub domain: String,
    /// Semicolon-separated domain aliases.
    pub aliases: String,
    /// Owner user id.
    pub owner_id: Uuid,
}

/// Mailbox record supplied to the GDPR exporter.
#[derive(Debug, Clone)]
pub struct GdprSourceMailbox {
    /// Mailbox address.
    pub address: String,
    /// Display name.
    pub display_name: String,
    /// Quota in bytes, when set.
    pub quota_bytes: Option<u64>,
}

/// Database record supplied to the GDPR exporter.
#[derive(Debug, Clone)]
pub struct GdprSourceDatabase {
    /// Database record id.
    pub id: Uuid,
    /// Database name.
    pub name: String,
    /// Owner user id.
    pub owner_id: Uuid,
}

/// API token record supplied to the GDPR exporter.
#[derive(Debug, Clone)]
pub struct GdprSourceApiToken {
    /// Token record id.
    pub id: Uuid,
    /// Token name.
    pub name: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
}

/// Source port the exporter uses to gather PII. The default
/// implementation returns empty lists; production wiring supplies a
/// concrete port that fans out to the sites / mail / databases /
/// api-tokens services.
#[async_trait::async_trait]
pub trait GdprSources: Send + Sync {
    /// Sites owned by `user_id`.
    async fn sites_for(&self, user_id: Uuid) -> Result<Vec<GdprSourceSite>, ComplianceError>;
    /// Mailboxes owned by `user_id`.
    async fn mail_for(&self, user_id: Uuid) -> Result<Vec<GdprSourceMailbox>, ComplianceError>;
    /// Databases owned by `user_id`.
    async fn databases_for(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<GdprSourceDatabase>, ComplianceError>;
    /// API tokens owned by `user_id`.
    async fn api_tokens_for(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<GdprSourceApiToken>, ComplianceError>;
}

/// Empty source port used by the default exporter. Tests provide
/// a fixture implementation; production code injects a fan-out
/// adapter that reads from the relevant bounded contexts.
struct EmptyGdprSources;

#[async_trait::async_trait]
impl GdprSources for EmptyGdprSources {
    async fn sites_for(&self, _: Uuid) -> Result<Vec<GdprSourceSite>, ComplianceError> {
        Ok(Vec::new())
    }

    async fn mail_for(&self, _: Uuid) -> Result<Vec<GdprSourceMailbox>, ComplianceError> {
        Ok(Vec::new())
    }

    async fn databases_for(&self, _: Uuid) -> Result<Vec<GdprSourceDatabase>, ComplianceError> {
        Ok(Vec::new())
    }

    async fn api_tokens_for(&self, _: Uuid) -> Result<Vec<GdprSourceApiToken>, ComplianceError> {
        Ok(Vec::new())
    }
}

fn require_admin(caller: &User) -> Result<(), ComplianceError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(ComplianceError::Forbidden),
    }
}

/// A profile description: a profile name and the set of rule ids
/// it contains.
pub struct ProfileSpec {
    /// Stable profile id (e.g. `cis-debian-12`).
    pub id: &'static str,
    /// CIS rule references.
    pub rules: &'static [&'static str],
}

/// Shipped profile definitions. The default profile covers a
/// conservative subset of CIS recommendations relevant to a single
/// host running OpenPanel.
pub const PROFILES: &[ProfileSpec] = &[ProfileSpec {
    id: "cis-debian-12-minimal",
    rules: &["5.2.1", "5.2.4", "5.2.6", "5.4.1"],
}];

/// Default profile id used when the caller does not specify one.
pub const DEFAULT_PROFILE: &str = "cis-debian-12-minimal";

/// Shipped CIS rule titles, used by the wizard to produce
/// human-readable output.
const RULE_TITLES: &[(&str, &str)] = &[
    (
        "5.2.1",
        "Ensure password creation requirements are configured",
    ),
    ("5.2.4", "Ensure password hashing algorithm is SHA-512"),
    ("5.2.6", "Ensure password reuse is limited"),
    ("5.4.1", "Ensure password expiration is configured"),
];

fn rules_for_profile(profile: &str) -> Vec<RuleSpec> {
    #[allow(clippy::expect_used)]
    // PROFILES is a compile-time non-empty const, so `first()` always yields a profile.
    let spec = PROFILES
        .iter()
        .find(|p| p.id == profile)
        .unwrap_or(PROFILES.first().expect("at least one profile"));
    spec.rules
        .iter()
        .map(|rule_id| RuleSpec {
            id: (*rule_id).to_string(),
            title: RULE_TITLES
                .iter()
                .find(|(id, _)| *id == *rule_id)
                .map(|(_, t)| (*t).to_string())
                .unwrap_or_else(|| rule_id.to_string()),
        })
        .collect()
}

struct RuleSpec {
    id: String,
    title: String,
}
