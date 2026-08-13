//! WAF application use cases.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, Site, SiteRepository, User,
    waf::{DryRunMatch, DryRunRequest, RuleSet, WafError, WafHit, WafRepository},
};
use serde::Serialize;
use uuid::Uuid;

use super::NginxSnippetCompiler;
use crate::sites::nginx::NginxConfigGenerator;

/// Port that atomically tests and activates a compiled site snippet.
#[async_trait]
pub trait WafConfigApplier: Send + Sync + 'static {
    /// Apply the snippet to the site's rendered nginx candidate.
    async fn apply(&self, site: &Site, snippet: &str) -> Result<(), WafError>;
}

/// Production applier backed by the sites nginx generator.
pub struct NginxWafConfigApplier {
    generator: NginxConfigGenerator,
}

impl NginxWafConfigApplier {
    /// Construct an applier over the shared nginx paths.
    pub fn new(generator: NginxConfigGenerator) -> Self {
        Self { generator }
    }
}

#[async_trait]
impl WafConfigApplier for NginxWafConfigApplier {
    async fn apply(&self, site: &Site, snippet: &str) -> Result<(), WafError> {
        self.generator
            .apply_with_waf(site, snippet)
            .map_err(|error| WafError::Compile(bounded(&error.to_string())))
    }
}

/// Dry-run response combining pure compilation and match simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DryRunResult {
    /// Compiled candidate snippet.
    pub snippet: String,
    /// Whether and how the rule matches the fixture.
    #[serde(flatten)]
    pub matched: DryRunMatch,
}

/// Owner-only per-site WAF orchestration service.
pub struct WafService {
    repo: Arc<dyn WafRepository>,
    sites: Arc<dyn SiteRepository>,
    audit: Arc<dyn AuditService>,
    applier: Arc<dyn WafConfigApplier>,
    spike_threshold: u64,
}

impl WafService {
    /// Construct with persistence, site lookup, audit, and config ports.
    pub fn new(
        repo: Arc<dyn WafRepository>,
        sites: Arc<dyn SiteRepository>,
        audit: Arc<dyn AuditService>,
        applier: Arc<dyn WafConfigApplier>,
    ) -> Self {
        Self {
            repo,
            sites,
            audit,
            applier,
            spike_threshold: 100,
        }
    }

    /// Override the per-sample spike threshold used to emit `AlertFired`.
    pub fn with_spike_threshold(mut self, threshold: u64) -> Self {
        self.spike_threshold = threshold.max(1);
        self
    }

    /// Get a site's rule set, returning an empty initial policy when absent.
    pub async fn get(&self, caller: &User, site_id: Uuid) -> Result<RuleSet, WafError> {
        owner(caller)?;
        self.require_site(site_id).await?;
        Ok(self
            .repo
            .get(site_id)
            .await
            .map_err(persistence)?
            .unwrap_or_else(|| RuleSet::empty(site_id)))
    }

    /// Atomically replace a complete rule set after nginx validation.
    pub async fn put(&self, caller: &User, desired: RuleSet) -> Result<RuleSet, WafError> {
        owner(caller)?;
        let desired = desired.validated()?;
        let site = self.require_site(desired.site_id()).await?;
        let current = self
            .repo
            .get(desired.site_id())
            .await
            .map_err(persistence)?;
        if current.as_ref() == Some(&desired) {
            return Ok(desired);
        }
        let snippet = NginxSnippetCompiler::compile(&desired)?;
        if let Err(error) = self.applier.apply(&site, &snippet).await {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::WafChanged,
                        AuditOutcome::Failure,
                    )
                    .target(desired.site_id().to_string())
                    .metadata(serde_json::json!({"reason": bounded(&error.to_string())})),
                )
                .await;
            return Err(error);
        }
        self.repo.put(&desired).await.map_err(persistence)?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::WafChanged,
                    AuditOutcome::Success,
                )
                .target(desired.site_id().to_string())
                .metadata(serde_json::json!({
                    "version": desired.version(),
                    "rules": desired.rules().len()
                })),
            )
            .await;
        Ok(desired)
    }

    /// Compile and simulate one rule without persistence or config writes.
    pub async fn dry_run(
        &self,
        caller: &User,
        site_id: Uuid,
        request: DryRunRequest,
    ) -> Result<DryRunResult, WafError> {
        owner(caller)?;
        self.require_site(site_id).await?;
        let matched = request.simulate()?;
        let snippet = NginxSnippetCompiler::compile_one(site_id, request.rule)?;
        Ok(DryRunResult { snippet, matched })
    }

    /// List per-rule hit totals for a site.
    pub async fn hits(&self, caller: &User, site_id: Uuid) -> Result<Vec<WafHit>, WafError> {
        owner(caller)?;
        self.require_site(site_id).await?;
        self.repo.hits(site_id).await.map_err(persistence)
    }

    /// Ingest one bounded sampler observation.
    pub async fn sample_hit(&self, hit: WafHit) -> Result<(), WafError> {
        if hit.count == 0
            || hit.kind.is_empty()
            || hit.kind.len() > 64
            || !hit
                .kind
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
        {
            return Err(WafError::Invalid("invalid WAF hit observation".into()));
        }
        self.repo.record_hit(&hit).await.map_err(persistence)?;
        if hit.count >= self.spike_threshold {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        "waf-sampler",
                        AuditAction::AlertFired,
                        AuditOutcome::Success,
                    )
                    .target(hit.site_id.to_string())
                    .metadata(serde_json::json!({
                        "metric": "waf.hits",
                        "kind": hit.kind,
                        "rule_id": hit.rule_id,
                        "action": hit.action,
                        "value": hit.count,
                        "threshold": self.spike_threshold
                    })),
                )
                .await;
        }
        Ok(())
    }

    async fn require_site(&self, site_id: Uuid) -> Result<Site, WafError> {
        self.sites
            .find_by_id(site_id)
            .await
            .map_err(persistence)?
            .ok_or_else(|| WafError::SiteNotFound(site_id.to_string()))
    }
}

fn owner(caller: &User) -> Result<(), WafError> {
    if caller.role() != Role::Owner {
        return Err(WafError::Forbidden);
    }
    Ok(())
}

fn persistence(error: impl std::fmt::Display) -> WafError {
    WafError::Persistence(bounded(&error.to_string()))
}

fn bounded(value: &str) -> String {
    value.chars().take(512).collect()
}
