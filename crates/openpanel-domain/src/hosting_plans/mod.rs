//! Hosting plans bounded context: plan definitions, quota caps,
//! feature toggles, plan resolution, and the assignment table.
//!
//! Plan definitions live in [`HostingPlan`]; the per-user effective
//! quota is computed by [`PlanResolver`]. The aggregation is built
//! so that the `resource-quotas` and `cron-role-permissions` changes
//! can consume [`PlanResolver::effective`] without re-implementing
//! the join or loading the plan on every request.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::RepoError;
pub use crate::hosting::HostingPlanId;

/// Stable identifier for a hosting plan. The reuse of the
/// `HostingPlanId` newtype from the placeholder module keeps the
/// FK on `users.hosting_plan_id` stable through this change.
pub type PlanId = HostingPlanId;

/// Stable identifier for a web-application installer entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AppId(pub Uuid);

impl AppId {
    /// Brand a uuid as an `AppId`.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Underlying UUID.
    pub fn as_uuid(&self) -> Uuid {
        self.0
    }
}

impl Default for AppId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AppId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Reference to a PHP runtime install (e.g. `php8.3-fpm`).
/// The version string is matched against the
/// `add-per-site-php-runtime` change's allowlist before the
/// runtime is provisioned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HostedPhpRuntimeRef(pub String);

impl HostedPhpRuntimeRef {
    /// Construct a runtime reference. The string is validated by
    /// [`HostedPhpRuntimeRef::parse`].
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Construct and validate. Returns `Noop` for an empty string
    /// and rejects strings that do not match the PHP runtime
    /// vocabulary used by the runtime installer.
    pub fn parse(value: &str) -> Result<Self, HostingPlansError> {
        if value.is_empty() {
            return Err(HostingPlansError::InvalidPhpRuntime(
                "runtime must not be empty".to_string(),
            ));
        }
        if value.len() > 64 {
            return Err(HostingPlansError::InvalidPhpRuntime(
                "runtime exceeds 64 chars".to_string(),
            ));
        }
        if !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
        {
            return Err(HostingPlansError::InvalidPhpRuntime(
                "runtime contains invalid characters".to_string(),
            ));
        }
        Ok(Self(value.to_string()))
    }

    /// Underlying runtime string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HostedPhpRuntimeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Lifecycle state of a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    /// Plan is available for new assignments.
    Active,
    /// Plan exists but refuses new assignments.
    Disabled,
}

/// Toggle state for a feature on a plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFeatureState {
    /// Feature is unavailable to a user on this plan.
    Disabled,
    /// Feature is available but optional.
    Optional,
    /// Feature is part of the plan and cannot be disabled.
    Required,
}

/// Catalog of feature toggles a plan can expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanFeature {
    /// Outbound SMTP for web applications.
    Smtp,
    /// Cron job creation.
    Cron,
    /// SSH/SFTP access.
    Sftp,
    /// Docker container management.
    Docker,
    /// API-token issuance.
    ApiTokens,
    /// Mailbox management.
    Mail,
    /// Database management.
    Databases,
    /// Backup scheduling.
    Backups,
    /// Web application installer.
    WebApps,
    /// Web application malware scanner.
    MalwareScanner,
}

/// Display-only price entry. The panel never charges — the price
/// is shown to the operator so it can be quoted to customers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanPrice {
    /// Amount in the smallest unit of `currency` (e.g. cents).
    pub amount_minor: u64,
    /// ISO 4217 currency code (e.g. `USD`).
    pub currency: String,
    /// Period label (e.g. `monthly`, `yearly`). Free-form.
    pub period: String,
}

impl PlanPrice {
    /// Build a validated price entry. The currency code must be
    /// 3 ASCII letters; the period must be non-empty and ≤ 32 chars.
    pub fn new(
        amount_minor: u64,
        currency: impl Into<String>,
        period: impl Into<String>,
    ) -> Result<Self, HostingPlansError> {
        let currency = currency.into();
        let period = period.into();
        if currency.len() != 3 || !currency.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(HostingPlansError::InvalidPrice(format!(
                "currency must be 3 uppercase ASCII letters, got `{currency}`"
            )));
        }
        if period.is_empty() || period.len() > 32 {
            return Err(HostingPlansError::InvalidPrice(format!(
                "period must be 1..=32 chars, got `{}`",
                period.len()
            )));
        }
        Ok(Self {
            amount_minor,
            currency,
            period,
        })
    }
}

/// Quota caps and feature limits a plan imposes on a user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanQuotas {
    /// Disk bytes the user may occupy.
    pub disk_bytes: u64,
    /// Outbound network bytes per calendar month.
    pub bandwidth_bytes_per_month: u64,
    /// Maximum number of sites the user may own.
    pub max_sites: u32,
    /// Maximum number of databases the user may own.
    pub max_databases: u32,
    /// Maximum mail domains the user may own.
    pub max_mail_domains: u32,
    /// Maximum mailboxes the user may own.
    pub max_mailboxes: u32,
    /// Maximum cron jobs the user may own.
    pub max_cron_jobs: u32,
    /// Maximum API tokens the user may own.
    pub max_api_tokens: u32,
    /// Maximum fleet agents the user may register.
    pub max_fleet_agents: u32,
}

impl PlanQuotas {
    /// Empty (zero) quota set. Useful for testing and for plans
    /// that only enable features without limits.
    pub fn zero() -> Self {
        Self {
            disk_bytes: 0,
            bandwidth_bytes_per_month: 0,
            max_sites: 0,
            max_databases: 0,
            max_mail_domains: 0,
            max_mailboxes: 0,
            max_cron_jobs: 0,
            max_api_tokens: 0,
            max_fleet_agents: 0,
        }
    }

    /// Compute the per-axis minimum of two quota sets. The result
    /// is what the user can actually consume given both sources.
    pub fn min(&self, other: &PlanQuotas) -> PlanQuotas {
        PlanQuotas {
            disk_bytes: self.disk_bytes.min(other.disk_bytes),
            bandwidth_bytes_per_month: self
                .bandwidth_bytes_per_month
                .min(other.bandwidth_bytes_per_month),
            max_sites: self.max_sites.min(other.max_sites),
            max_databases: self.max_databases.min(other.max_databases),
            max_mail_domains: self.max_mail_domains.min(other.max_mail_domains),
            max_mailboxes: self.max_mailboxes.min(other.max_mailboxes),
            max_cron_jobs: self.max_cron_jobs.min(other.max_cron_jobs),
            max_api_tokens: self.max_api_tokens.min(other.max_api_tokens),
            max_fleet_agents: self.max_fleet_agents.min(other.max_fleet_agents),
        }
    }
}

/// The aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostingPlan {
    id: PlanId,
    name: String,
    description: String,
    prices: Vec<PlanPrice>,
    features: BTreeMap<PlanFeature, PlanFeatureState>,
    quota_caps: PlanQuotas,
    allowed_apps: Vec<AppId>,
    allowed_php_runtimes: Vec<HostedPhpRuntimeRef>,
    status: PlanStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl HostingPlan {
    /// Construct a new plan. The name is validated.
    pub fn new(
        id: PlanId,
        name: impl Into<String>,
        description: impl Into<String>,
        quota_caps: PlanQuotas,
    ) -> Result<Self, HostingPlansError> {
        let name = name.into();
        let description = description.into();
        if name.len() < 3 || name.len() > 64 {
            return Err(HostingPlansError::InvalidName(format!(
                "name must be 3..=64 chars, got {}",
                name.len()
            )));
        }
        Ok(Self {
            id,
            name,
            description,
            prices: Vec::new(),
            features: BTreeMap::new(),
            quota_caps,
            allowed_apps: Vec::new(),
            allowed_php_runtimes: Vec::new(),
            status: PlanStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    /// Identifier.
    pub fn id(&self) -> PlanId {
        self.id
    }

    /// Display name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Free-form description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Display-only prices.
    pub fn prices(&self) -> &[PlanPrice] {
        &self.prices
    }

    /// Feature toggles.
    pub fn features(&self) -> &BTreeMap<PlanFeature, PlanFeatureState> {
        &self.features
    }

    /// Quota caps.
    pub fn quota_caps(&self) -> &PlanQuotas {
        &self.quota_caps
    }

    /// Allowed web application ids.
    pub fn allowed_apps(&self) -> &[AppId] {
        &self.allowed_apps
    }

    /// Allowed PHP runtime versions.
    pub fn allowed_php_runtimes(&self) -> &[HostedPhpRuntimeRef] {
        &self.allowed_php_runtimes
    }

    /// Lifecycle status.
    pub fn status(&self) -> PlanStatus {
        self.status
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Last-mutation timestamp.
    pub fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    /// Replace the price list. Each entry is validated.
    pub fn set_prices(&mut self, prices: Vec<PlanPrice>) -> Result<(), HostingPlansError> {
        for price in &prices {
            if price.currency.len() != 3 || !price.currency.bytes().all(|b| b.is_ascii_uppercase())
            {
                return Err(HostingPlansError::InvalidPrice(format!(
                    "currency must be 3 uppercase ASCII letters, got `{}`",
                    price.currency
                )));
            }
        }
        self.prices = prices;
        self.touch();
        Ok(())
    }

    /// Set the feature toggle for a single feature.
    pub fn set_feature(&mut self, feature: PlanFeature, state: PlanFeatureState) {
        self.features.insert(feature, state);
        self.touch();
    }

    /// Replace the quota caps. The caller MUST have already
    /// checked that the new caps do not shrink below the
    /// currently-assigned user consumption (see
    /// [`HostingPlansError::WouldShrinkBelowUsage`]).
    pub fn set_quota_caps(&mut self, caps: PlanQuotas) {
        self.quota_caps = caps;
        self.touch();
    }

    /// Replace the description.
    pub fn set_description(&mut self, description: impl Into<String>) {
        self.description = description.into();
        self.touch();
    }

    /// Replace the allowed-app list.
    pub fn set_allowed_apps(&mut self, apps: Vec<AppId>) {
        self.allowed_apps = apps;
        self.touch();
    }

    /// Replace the allowed PHP runtime list. Each entry is
    /// validated through [`HostedPhpRuntimeRef::parse`].
    pub fn set_allowed_php_runtimes(
        &mut self,
        runtimes: Vec<String>,
    ) -> Result<(), HostingPlansError> {
        let mut parsed = Vec::with_capacity(runtimes.len());
        for value in runtimes {
            parsed.push(HostedPhpRuntimeRef::parse(&value)?);
        }
        self.allowed_php_runtimes = parsed;
        self.touch();
        Ok(())
    }

    /// Move the plan to `Disabled`. Disabled plans refuse new
    /// assignments but still resolve the existing ones.
    pub fn disable(&mut self) {
        self.status = PlanStatus::Disabled;
        self.touch();
    }

    /// Re-enable a previously disabled plan. The plan name must
    /// not have been reused by another plan (the service enforces
    /// this; the aggregate carries no such lookup).
    pub fn enable(&mut self) {
        self.status = PlanStatus::Active;
        self.touch();
    }

    /// Whether the plan accepts new assignments.
    pub fn accepts_assignments(&self) -> bool {
        matches!(self.status, PlanStatus::Active)
    }

    /// Build a clone of this plan with a new id and a fresh name.
    /// Used by the `clone` endpoint. Created by the service so
    /// timestamps are reset.
    pub fn clone_with_name(
        &self,
        new_id: PlanId,
        new_name: impl Into<String>,
    ) -> Result<Self, HostingPlansError> {
        let mut clone = Self::new(
            new_id,
            new_name,
            self.description.clone(),
            self.quota_caps.clone(),
        )?;
        clone.prices = self.prices.clone();
        clone.features = self.features.clone();
        clone.allowed_apps = self.allowed_apps.clone();
        clone.allowed_php_runtimes = self.allowed_php_runtimes.clone();
        Ok(clone)
    }

    /// Restore from persistence. Used by the SQLite adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: PlanId,
        name: String,
        description: String,
        prices_json: &str,
        features_json: &str,
        quota_caps: PlanQuotas,
        allowed_apps_json: &str,
        allowed_php_runtimes_json: &str,
        status: PlanStatus,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, HostingPlansError> {
        let prices: Vec<PlanPrice> = serde_json::from_str(prices_json)
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let features: BTreeMap<PlanFeature, PlanFeatureState> = serde_json::from_str(features_json)
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let allowed_apps: Vec<Uuid> = serde_json::from_str(allowed_apps_json)
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let allowed_php_runtimes: Vec<String> = serde_json::from_str(allowed_php_runtimes_json)
            .map_err(|e| HostingPlansError::Persistence(e.to_string()))?;
        let mut parsed_runtimes = Vec::with_capacity(allowed_php_runtimes.len());
        for value in allowed_php_runtimes {
            parsed_runtimes.push(HostedPhpRuntimeRef::parse(&value)?);
        }
        Ok(Self {
            id,
            name,
            description,
            prices,
            features,
            quota_caps,
            allowed_apps: allowed_apps.into_iter().map(AppId).collect(),
            allowed_php_runtimes: parsed_runtimes,
            status,
            created_at,
            updated_at,
        })
    }

    /// Persisted price list as JSON.
    pub fn prices_json(&self) -> String {
        serde_json::to_string(&self.prices).unwrap_or_else(|_| "[]".to_string())
    }

    /// Persisted feature map as JSON.
    pub fn features_json(&self) -> String {
        serde_json::to_string(&self.features).unwrap_or_else(|_| "{}".to_string())
    }

    /// Persisted allowed apps as JSON.
    pub fn allowed_apps_json(&self) -> String {
        serde_json::to_string(
            &self
                .allowed_apps
                .iter()
                .map(|a| a.as_uuid())
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_string())
    }

    /// Persisted allowed PHP runtimes as JSON.
    pub fn allowed_php_runtimes_json(&self) -> String {
        serde_json::to_string(
            &self
                .allowed_php_runtimes
                .iter()
                .map(|r| r.as_str().to_string())
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| "[]".to_string())
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

/// A current assignment from a user to a plan. The table is
/// append-only: re-assignment inserts a new row and marks the
/// previous one as `replaced_at`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanAssignment {
    user_id: Uuid,
    plan_id: PlanId,
    assigned_at: DateTime<Utc>,
    assigned_by: Uuid,
    replaced_at: Option<DateTime<Utc>>,
}

impl PlanAssignment {
    /// Build a new assignment row.
    pub fn new(
        user_id: Uuid,
        plan_id: PlanId,
        assigned_by: Uuid,
        assigned_at: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            plan_id,
            assigned_at,
            assigned_by,
            replaced_at: None,
        }
    }

    /// Build a restored row from persistence.
    pub fn restore(
        user_id: Uuid,
        plan_id: PlanId,
        assigned_at: DateTime<Utc>,
        assigned_by: Uuid,
        replaced_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            user_id,
            plan_id,
            assigned_at,
            assigned_by,
            replaced_at,
        }
    }

    /// User id.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Plan id.
    pub fn plan_id(&self) -> PlanId {
        self.plan_id
    }

    /// When the assignment was made.
    pub fn assigned_at(&self) -> DateTime<Utc> {
        self.assigned_at
    }

    /// Who made the assignment.
    pub fn assigned_by(&self) -> Uuid {
        self.assigned_by
    }

    /// When the assignment was replaced (None = current).
    pub fn replaced_at(&self) -> Option<DateTime<Utc>> {
        self.replaced_at
    }

    /// Whether the assignment is current.
    pub fn is_current(&self) -> bool {
        self.replaced_at.is_none()
    }
}

/// Per-axis consumption of plan-driven limits. The resolver
/// returns this when a downstream feature (e.g. `resource-quotas`)
/// needs to know how much of `disk_bytes` the user may write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EffectiveQuotas {
    /// Disk bytes the user may occupy.
    pub disk_bytes: u64,
    /// Outbound network bytes per calendar month.
    pub bandwidth_bytes_per_month: u64,
    /// Maximum number of sites the user may own.
    pub max_sites: u32,
    /// Maximum number of databases the user may own.
    pub max_databases: u32,
    /// Maximum mail domains the user may own.
    pub max_mail_domains: u32,
    /// Maximum mailboxes the user may own.
    pub max_mailboxes: u32,
    /// Maximum cron jobs the user may own.
    pub max_cron_jobs: u32,
    /// Maximum API tokens the user may own.
    pub max_api_tokens: u32,
    /// Maximum fleet agents the user may register.
    pub max_fleet_agents: u32,
}

impl EffectiveQuotas {
    /// Convert a resolved [`PlanQuotas`] into [`EffectiveQuotas`] for
    /// downstream consumers.
    pub fn from_caps(caps: PlanQuotas) -> Self {
        Self {
            disk_bytes: caps.disk_bytes,
            bandwidth_bytes_per_month: caps.bandwidth_bytes_per_month,
            max_sites: caps.max_sites,
            max_databases: caps.max_databases,
            max_mail_domains: caps.max_mail_domains,
            max_mailboxes: caps.max_mailboxes,
            max_cron_jobs: caps.max_cron_jobs,
            max_api_tokens: caps.max_api_tokens,
            max_fleet_agents: caps.max_fleet_agents,
        }
    }
}

/// Read-side resolver for the effective cap a user is entitled to.
/// The implementation is owned by the application layer; the trait
/// lives in the domain so it can be passed to other bounded contexts
/// (resource-quotas, cron-role-permissions) without depending on
/// the SQLite adapter.
pub trait PlanResolver: Send + Sync + 'static {
    /// Compute the per-axis effective caps for a user. The
    /// resolver MAY cache for up to 60 seconds; the cache TTL is
    /// owned by the application-layer implementation.
    fn effective(&self, user_id: Uuid) -> EffectiveQuotas;
}

/// Persistence port for plans and assignments.
#[async_trait]
pub trait HostingPlanRepository: Send + Sync + 'static {
    /// Insert a new plan. The implementation MUST enforce name
    /// uniqueness and return `HostingPlansError::DuplicateName`.
    async fn insert(&self, plan: &HostingPlan) -> Result<(), HostingPlansError>;
    /// Find a plan by id.
    async fn find_by_id(&self, id: PlanId) -> Result<Option<HostingPlan>, HostingPlansError>;
    /// Find a plan by name.
    async fn find_by_name(&self, name: &str) -> Result<Option<HostingPlan>, HostingPlansError>;
    /// List all plans (active and disabled).
    async fn list(&self) -> Result<Vec<HostingPlan>, HostingPlansError>;
    /// Replace an existing plan row.
    async fn update(&self, plan: &HostingPlan) -> Result<(), HostingPlansError>;
    /// Delete a plan by id. The implementation MUST refuse if any
    /// current assignment exists and return `PlanInUse`.
    async fn delete(&self, id: PlanId) -> Result<(), HostingPlansError>;
    /// Insert a new assignment row. The implementation MUST set
    /// `replaced_at = now` on the user's prior current row in the
    /// same transaction.
    async fn insert_assignment(&self, assignment: &PlanAssignment)
    -> Result<(), HostingPlansError>;
    /// Mark the user's current assignment as `replaced_at = now`.
    /// Returns the plan id that was previously assigned, if any.
    async fn replace_current_assignment(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError>;
    /// Remove the current assignment for a user.
    async fn remove_current_assignment(
        &self,
        user_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Option<PlanId>, HostingPlansError>;
    /// Return the current assignment for a user, if any.
    async fn current_assignment(
        &self,
        user_id: Uuid,
    ) -> Result<Option<PlanAssignment>, HostingPlansError>;
    /// Count the current assignments for a plan. Used by the
    /// delete path to refuse deletion of plans in use.
    async fn count_assignments(&self, plan_id: PlanId) -> Result<i64, HostingPlansError>;
    /// Provide a default implementation of the placeholder
    /// repository used by the
    /// `refine-identity-with-hierarchy-and-plan-fields` change.
    /// It returns `false` so the identity `find_by_plan` looks
    /// up nothing until the full plan module ships.
    async fn exists(&self, id: PlanId) -> Result<bool, RepoError> {
        let _ = id;
        Ok(false)
    }
}

/// Errors that can occur in the hosting-plans bounded context.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HostingPlansError {
    /// A plan with the same name already exists.
    #[error("plan name already in use")]
    DuplicateName,
    /// The plan is currently disabled and cannot accept new
    /// assignments.
    #[error("plan is disabled")]
    PlanDisabled,
    /// The plan cannot be deleted because at least one user is
    /// currently assigned.
    #[error("plan is in use")]
    PlanInUse,
    /// The plan is not found.
    #[error("plan not found")]
    PlanNotFound,
    /// The plan name is invalid.
    #[error("invalid plan name: {0}")]
    InvalidName(String),
    /// The price entry is invalid.
    #[error("invalid plan price: {0}")]
    InvalidPrice(String),
    /// The PHP runtime reference is invalid.
    #[error("invalid php runtime: {0}")]
    InvalidPhpRuntime(String),
    /// The user is not found.
    #[error("user not found")]
    UserNotFound,
    /// The requested update would shrink quota caps below the
    /// currently-assigned user consumption.
    #[error("would shrink quota cap below assigned usage")]
    WouldShrinkBelowUsage,
    /// Persistence layer failure.
    #[error("hosting plan persistence error: {0}")]
    Persistence(String),
}

impl From<RepoError> for HostingPlansError {
    fn from(error: RepoError) -> Self {
        HostingPlansError::Persistence(error.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_caps() -> PlanQuotas {
        PlanQuotas {
            disk_bytes: 10 * 1024 * 1024 * 1024,
            bandwidth_bytes_per_month: 100 * 1024 * 1024 * 1024,
            max_sites: 5,
            max_databases: 5,
            max_mail_domains: 2,
            max_mailboxes: 10,
            max_cron_jobs: 25,
            max_api_tokens: 4,
            max_fleet_agents: 0,
        }
    }

    #[test]
    fn new_rejects_undersized_name() {
        let id = PlanId::new();
        let err = HostingPlan::new(id, "ab", "tiny", sample_caps()).expect_err("must reject");
        assert!(matches!(err, HostingPlansError::InvalidName(_)));
    }

    #[test]
    fn new_rejects_oversized_name() {
        let id = PlanId::new();
        let name = "a".repeat(65);
        let err = HostingPlan::new(id, name, "too long", sample_caps()).expect_err("must reject");
        assert!(matches!(err, HostingPlansError::InvalidName(_)));
    }

    #[test]
    fn new_accepts_valid_name() {
        let id = PlanId::new();
        let plan = HostingPlan::new(id, "Basic", "starter", sample_caps()).expect("ok");
        assert_eq!(plan.name(), "Basic");
        assert!(plan.accepts_assignments());
    }

    #[test]
    fn set_quota_caps_round_trips() {
        let mut plan = HostingPlan::new(PlanId::new(), "Basic", "starter", sample_caps()).unwrap();
        let new_caps = PlanQuotas::zero();
        plan.set_quota_caps(new_caps.clone());
        assert_eq!(plan.quota_caps(), &new_caps);
    }

    #[test]
    fn disable_then_enable_round_trips() {
        let mut plan = HostingPlan::new(PlanId::new(), "Basic", "starter", sample_caps()).unwrap();
        plan.disable();
        assert!(!plan.accepts_assignments());
        plan.enable();
        assert!(plan.accepts_assignments());
    }

    #[test]
    fn set_prices_rejects_bad_currency() {
        let mut plan = HostingPlan::new(PlanId::new(), "Basic", "starter", sample_caps()).unwrap();
        let err = plan
            .set_prices(vec![PlanPrice {
                amount_minor: 100,
                currency: "usd".to_string(), // lowercase
                period: "monthly".to_string(),
            }])
            .expect_err("currency must be uppercase");
        assert!(matches!(err, HostingPlansError::InvalidPrice(_)));
    }

    #[test]
    fn set_php_runtimes_validates_each_entry() {
        let mut plan = HostingPlan::new(PlanId::new(), "Basic", "starter", sample_caps()).unwrap();
        let res = plan.set_allowed_php_runtimes(vec!["php8.3".into(), "bad runtime!".into()]);
        assert!(matches!(res, Err(HostingPlansError::InvalidPhpRuntime(_))));
    }

    #[test]
    fn min_returns_per_axis_minimum() {
        let a = PlanQuotas {
            disk_bytes: 1000,
            bandwidth_bytes_per_month: 2000,
            max_sites: 5,
            max_databases: 5,
            max_mail_domains: 2,
            max_mailboxes: 10,
            max_cron_jobs: 25,
            max_api_tokens: 4,
            max_fleet_agents: 0,
        };
        let b = PlanQuotas {
            disk_bytes: 700,
            bandwidth_bytes_per_month: 5000,
            max_sites: 3,
            max_databases: 10,
            max_mail_domains: 5,
            max_mailboxes: 2,
            max_cron_jobs: 10,
            max_api_tokens: 9,
            max_fleet_agents: 2,
        };
        let min = a.min(&b);
        assert_eq!(min.disk_bytes, 700);
        assert_eq!(min.bandwidth_bytes_per_month, 2000);
        assert_eq!(min.max_sites, 3);
        assert_eq!(min.max_databases, 5);
    }

    #[test]
    fn clone_with_name_creates_independent_aggregate() {
        let plan = HostingPlan::new(PlanId::new(), "Basic", "starter", sample_caps()).unwrap();
        let clone = plan.clone_with_name(PlanId::new(), "Plus").unwrap();
        assert_eq!(clone.name(), "Plus");
        assert_eq!(plan.name(), "Basic");
        assert_ne!(clone.id(), plan.id());
    }
}
