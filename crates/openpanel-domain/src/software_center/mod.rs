//! Pure domain invariants for the curated Software Center.

mod recipe;

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    path::{Component, Path},
};

pub use recipe::{
    ArtifactPin, CatalogEntryRecipe, CatalogHit, CatalogManifest, CatalogQuery, CatalogSearchPage,
    CatalogSort, Category, EntryKind, Homepage, License, Provenance, RecipeError, Tag, VersionSpec,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

/// Software Center value or lifecycle error.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SoftwareCenterError {
    /// A caller-provided or catalog-provided value is invalid.
    #[error("invalid software center value")]
    Invalid,
    /// A dependency edge would introduce a cycle.
    #[error("software dependency cycle")]
    DependencyCycle,
    /// A job transition is not valid from its current state.
    #[error("invalid software job transition")]
    InvalidTransition,
}

/// Stable lower-case identifier for a catalog entry.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CatalogId(String);
impl CatalogId {
    /// Validate a catalog identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, SoftwareCenterError> {
        validate_identifier(value.as_ref(), 64).map(Self)
    }

    /// Identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Fixed package-manager identifier, never a command fragment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PackageId(String);
impl PackageId {
    /// Validate one package identifier.
    pub fn new(value: impl AsRef<str>) -> Result<Self, SoftwareCenterError> {
        validate_identifier(value.as_ref(), 128).map(Self)
    }

    /// Package identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn validate_identifier(value: &str, maximum: usize) -> Result<String, SoftwareCenterError> {
    if value.is_empty()
        || value.len() > maximum
        || !value.is_ascii()
        || !value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'+' | b'-')
        })
    {
        return Err(SoftwareCenterError::Invalid);
    }
    Ok(value.to_owned())
}

/// Numeric semantic software version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SoftwareVersion {
    major: u32,
    minor: u32,
    patch: u32,
}
impl SoftwareVersion {
    /// Parse `major.minor` or `major.minor.patch` without floating aliases.
    pub fn parse(value: &str) -> Result<Self, SoftwareCenterError> {
        let parts: Vec<&str> = value.split('.').collect();
        if !(2..=3).contains(&parts.len())
            || parts
                .iter()
                .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(SoftwareCenterError::Invalid);
        }
        let major = parse_version_part(parts[0])?;
        let minor = parse_version_part(parts[1])?;
        let patch = match parts.get(2) {
            Some(value) => parse_version_part(value)?,
            None => 0,
        };
        Ok(Self {
            major,
            minor,
            patch,
        })
    }

    /// Major version number.
    pub fn major(self) -> u32 {
        self.major
    }
}

fn parse_version_part(value: &str) -> Result<u32, SoftwareCenterError> {
    if value.len() > 1 && value.starts_with('0') {
        return Err(SoftwareCenterError::Invalid);
    }
    value.parse().map_err(|_| SoftwareCenterError::Invalid)
}

/// Exact operating-system release and CPU architecture supported by a recipe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportedPlatform {
    distribution: String,
    release: String,
    architecture: String,
}
impl SupportedPlatform {
    /// Validate a supported platform tuple.
    pub fn new(
        distribution: impl AsRef<str>,
        release: impl AsRef<str>,
        architecture: impl AsRef<str>,
    ) -> Result<Self, SoftwareCenterError> {
        let distribution = validate_identifier(distribution.as_ref(), 32)?;
        let release = release.as_ref();
        if release.is_empty()
            || release.len() > 16
            || !release
                .bytes()
                .all(|byte| byte.is_ascii_digit() || byte == b'.')
        {
            return Err(SoftwareCenterError::Invalid);
        }
        let architecture = architecture.as_ref();
        if architecture.is_empty()
            || architecture.len() > 32
            || !architecture.is_ascii()
            || !architecture.bytes().all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
        {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(Self {
            distribution,
            release: release.to_owned(),
            architecture: architecture.to_owned(),
        })
    }

    /// Distribution identifier.
    pub fn distribution(&self) -> &str {
        &self.distribution
    }

    /// Distribution release identifier.
    pub fn release(&self) -> &str {
        &self.release
    }

    /// CPU architecture identifier.
    pub fn architecture(&self) -> &str {
        &self.architecture
    }
}

/// One fixed package operation in an immutable plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", content = "package", rename_all = "snake_case")]
pub enum PlanAction {
    /// Install one package.
    Install(PackageId),
    /// Update one package.
    Update(PackageId),
    /// Remove one package.
    Remove(PackageId),
}
impl PlanAction {
    /// Construct an installation action.
    pub fn install(package: PackageId) -> Self {
        Self::Install(package)
    }
}

/// Deterministic plan bound to catalog and platform state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwarePlan {
    catalog_digest: String,
    platform: SupportedPlatform,
    actions: Vec<PlanAction>,
    digest: String,
}
impl SoftwarePlan {
    /// Construct and hash a non-empty plan.
    pub fn new(
        catalog_digest: impl Into<String>,
        platform: SupportedPlatform,
        actions: Vec<PlanAction>,
    ) -> Result<Self, SoftwareCenterError> {
        let catalog_digest = catalog_digest.into();
        if catalog_digest.is_empty()
            || catalog_digest.len() > 128
            || !catalog_digest
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(SoftwareCenterError::Invalid);
        }
        let canonical = serde_json::to_vec(&(&catalog_digest, &platform, &actions))
            .map_err(|_| SoftwareCenterError::Invalid)?;
        let digest = hex::encode(Sha256::digest(canonical));
        Ok(Self {
            catalog_digest,
            platform,
            actions,
            digest,
        })
    }

    /// SHA-256 plan digest used for confirmation binding.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

/// One-use short-lived plan confirmation.
#[derive(Debug, Clone)]
pub struct ConfirmationToken {
    plan_digest: String,
    expires_at: u64,
    used: bool,
}
impl ConfirmationToken {
    /// Issue a token for at most five minutes.
    pub fn issue(
        plan_digest: impl Into<String>,
        now: u64,
        ttl: u64,
    ) -> Result<Self, SoftwareCenterError> {
        let plan_digest = plan_digest.into();
        if plan_digest.is_empty() || ttl == 0 || ttl > 300 {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(Self {
            plan_digest,
            expires_at: now.checked_add(ttl).ok_or(SoftwareCenterError::Invalid)?,
            used: false,
        })
    }

    /// Consume exactly once when scope and expiry match.
    pub fn consume(&mut self, plan_digest: &str, now: u64) -> bool {
        if self.used || self.plan_digest != plan_digest || now > self.expires_at {
            return false;
        }
        self.used = true;
        true
    }
}

/// Acyclic component dependency graph.
#[derive(Debug, Default, Clone)]
pub struct DependencyGraph {
    edges: BTreeMap<String, BTreeSet<String>>,
}
impl DependencyGraph {
    /// Add a component-to-dependency edge if it remains acyclic.
    pub fn add(
        &mut self,
        component: impl Into<String>,
        dependency: impl Into<String>,
    ) -> Result<(), SoftwareCenterError> {
        let component = component.into();
        let dependency = dependency.into();
        CatalogId::new(&component)?;
        CatalogId::new(&dependency)?;
        if component == dependency || self.reaches(&dependency, &component) {
            return Err(SoftwareCenterError::DependencyCycle);
        }
        self.edges.entry(component).or_default().insert(dependency);
        Ok(())
    }

    /// Return direct and transitive dependents in stable order.
    pub fn dependents_of(&self, dependency: &str) -> Vec<String> {
        let mut found = BTreeSet::new();
        let mut queue = VecDeque::from([dependency.to_owned()]);
        while let Some(target) = queue.pop_front() {
            for (component, dependencies) in &self.edges {
                if dependencies.contains(&target) && found.insert(component.clone()) {
                    queue.push_back(component.clone());
                }
            }
        }
        found.into_iter().collect()
    }

    fn reaches(&self, start: &str, target: &str) -> bool {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            if node == target {
                return true;
            }
            if seen.insert(node)
                && let Some(next) = self.edges.get(node)
            {
                queue.extend(next.iter().map(String::as_str));
            }
        }
        false
    }
}

/// Durable software job state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Waiting for the exclusive transaction lock.
    Queued,
    /// Package or application operations are active.
    Running,
    /// Post-change validation is active.
    Validating,
    /// Rollback is active.
    RollingBack,
    /// All changes validated.
    Succeeded,
    /// The transaction failed.
    Failed,
    /// The job stopped at a safe cancellation point.
    Cancelled,
    /// Recipe-supported reverse operations completed after interruption.
    RolledBack,
    /// Startup reconciliation found an abandoned active job.
    Interrupted,
}
impl JobState {
    fn terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::RolledBack | Self::Interrupted
        )
    }
}

/// Software transaction job aggregate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareJob {
    id: Uuid,
    plan_digest: String,
    state: JobState,
}
impl SoftwareJob {
    /// Create a queued job for an immutable plan.
    pub fn new(id: Uuid, plan_digest: impl Into<String>) -> Result<Self, SoftwareCenterError> {
        let plan_digest = plan_digest.into();
        if plan_digest.is_empty() {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(Self {
            id,
            plan_digest,
            state: JobState::Queued,
        })
    }

    /// Begin execution.
    pub fn start(&mut self) -> Result<(), SoftwareCenterError> {
        self.transition(JobState::Queued, JobState::Running)
    }

    /// Begin post-install validation.
    pub fn validate(&mut self) -> Result<(), SoftwareCenterError> {
        self.transition(JobState::Running, JobState::Validating)
    }

    /// Complete a validated transaction.
    pub fn succeed(&mut self) -> Result<(), SoftwareCenterError> {
        self.transition(JobState::Validating, JobState::Succeeded)
    }

    /// Fail an active transaction.
    pub fn fail(&mut self) -> Result<(), SoftwareCenterError> {
        if self.state.terminal() || self.state == JobState::Queued {
            return Err(SoftwareCenterError::InvalidTransition);
        }
        self.state = JobState::Failed;
        Ok(())
    }

    /// Reconcile an abandoned running, validating, or rollback job.
    pub fn reconcile_interrupted(&mut self) -> Result<(), SoftwareCenterError> {
        if !matches!(
            self.state,
            JobState::Running | JobState::Validating | JobState::RollingBack
        ) {
            return Err(SoftwareCenterError::InvalidTransition);
        }
        self.state = JobState::Interrupted;
        Ok(())
    }

    /// Current job state.
    pub fn state(&self) -> JobState {
        self.state
    }

    fn transition(
        &mut self,
        expected: JobState,
        next: JobState,
    ) -> Result<(), SoftwareCenterError> {
        if self.state != expected {
            return Err(SoftwareCenterError::InvalidTransition);
        }
        self.state = next;
        Ok(())
    }
}

/// Limits enforced while streaming an application archive into staging.
#[derive(Debug, Clone, Copy)]
pub struct ArchivePolicy {
    maximum_files: u64,
    maximum_expanded_bytes: u64,
}
impl ArchivePolicy {
    /// Construct positive extraction limits.
    pub fn new(
        maximum_files: u64,
        maximum_expanded_bytes: u64,
    ) -> Result<Self, SoftwareCenterError> {
        if maximum_files == 0 || maximum_expanded_bytes == 0 {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(Self {
            maximum_files,
            maximum_expanded_bytes,
        })
    }

    /// Validate one entry and cumulative counters before writing it.
    pub fn accept(
        self,
        path: &str,
        symlink: bool,
        expanded_bytes: u64,
        file_count: u64,
    ) -> Result<(), SoftwareCenterError> {
        let safe_path = !path.is_empty()
            && !path.contains('\\')
            && !path.chars().any(char::is_control)
            && Path::new(path)
                .components()
                .all(|component| matches!(component, Component::Normal(_)));
        if !safe_path
            || symlink
            || expanded_bytes > self.maximum_expanded_bytes
            || file_count > self.maximum_files
        {
            return Err(SoftwareCenterError::Invalid);
        }
        Ok(())
    }
}
