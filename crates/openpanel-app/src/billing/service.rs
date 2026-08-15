//! Reseller billing services: usage exporter, chargeback engine,
//! and webhook relay.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    BillingError, BillingRepository, BillingStatus, Chargeback, Integration, Role, UsageMeter,
    UsageUnit, User, compute_chargeback, hmac_sha256_hex, verify_signature,
};
use uuid::Uuid;

use crate::billing::SqliteBillingRepository;

/// Usage exporter: returns the list of meters for one owner.
pub struct UsageExporter {
    repo: Arc<SqliteBillingRepository>,
}

impl UsageExporter {
    /// Construct a usage exporter.
    pub fn new(repo: Arc<SqliteBillingRepository>) -> Self {
        Self { repo }
    }

    /// Export usage for `owner_id`.
    pub async fn export(
        &self,
        caller: &User,
        owner_id: Uuid,
    ) -> Result<Vec<UsageMeter>, BillingError> {
        require_admin(caller)?;
        Ok(self.repo.list_meters(owner_id).await?)
    }
}

/// Chargeback engine: persists a chargeback computed from a
/// owner's meters.
pub struct ChargebackEngine {
    repo: Arc<SqliteBillingRepository>,
    audit: Arc<dyn AuditService>,
}

impl ChargebackEngine {
    /// Construct a chargeback engine.
    pub fn new(repo: Arc<SqliteBillingRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Compute and persist a chargeback for `owner_id`. The
    /// caller supplies a list of unit prices (minor units) used
    /// to price the meter lines.
    pub async fn compute(
        &self,
        caller: &User,
        owner_id: Uuid,
        prices: &[(UsageUnit, u64)],
        currency: &str,
    ) -> Result<Chargeback, BillingError> {
        require_admin(caller)?;
        let meters = self.repo.list_meters(owner_id).await?;
        let period_start = meters
            .iter()
            .map(|m| m.period_start)
            .min()
            .unwrap_or_else(Utc::now);
        let period_end = meters
            .iter()
            .map(|m| m.period_end)
            .max()
            .unwrap_or_else(Utc::now);
        let chargeback = compute_chargeback(
            owner_id,
            period_start,
            period_end,
            &meters,
            prices,
            currency,
        );
        self.repo.save_chargeback(&chargeback).await?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::BillingChargebackComputed,
                    AuditOutcome::Success,
                )
                .target(owner_id.to_string())
                .metadata(serde_json::json!({
                    "amount_minor": chargeback.amount_minor,
                    "currency": chargeback.currency,
                })),
            )
            .await;
        Ok(chargeback)
    }
}

/// Outcome of a webhook relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayOutcome {
    /// Whether the relay was successful.
    pub relayed: bool,
    /// Reason when the relay was rejected.
    pub reason: Option<String>,
}

/// Webhook relay: verifies a signature, rejects the call when
/// the integration is disabled, and (in production) posts the
/// body to the integration's webhook URL.
pub struct WebhookRelay {
    repo: Arc<SqliteBillingRepository>,
    audit: Arc<dyn AuditService>,
}

impl WebhookRelay {
    /// Construct a relay.
    pub fn new(repo: Arc<SqliteBillingRepository>, audit: Arc<dyn AuditService>) -> Self {
        Self { repo, audit }
    }

    /// Verify a signature against the named integration. Refuses
    /// when the integration is disabled, missing, or the
    /// signature does not match.
    pub async fn verify(
        &self,
        caller: &User,
        integration_id: Uuid,
        body: &[u8],
        presented_signature: Option<&str>,
    ) -> Result<RelayOutcome, BillingError> {
        require_admin(caller)?;
        let integration = self
            .repo
            .get_integration(integration_id)
            .await?
            .ok_or(BillingError::NotFound(integration_id.to_string()))?;
        if integration.status != BillingStatus::Enabled {
            let reason = format!("integration {} is disabled", integration.name);
            self.audit
                .record(
                    AuditEvent::new(
                        caller.username().as_str(),
                        AuditAction::BillingWebhookRejected,
                        AuditOutcome::Denied,
                    )
                    .target(integration.id.to_string())
                    .metadata(serde_json::json!({ "reason": "disabled" })),
                )
                .await;
            return Ok(RelayOutcome {
                relayed: false,
                reason: Some(reason),
            });
        }
        verify_signature(&integration.webhook_secret, body, presented_signature)?;
        self.audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::BillingWebhookAccepted,
                    AuditOutcome::Success,
                )
                .target(integration.id.to_string()),
            )
            .await;
        Ok(RelayOutcome {
            relayed: true,
            reason: None,
        })
    }

    /// Sign a body for an integration. Used by the production
    /// caller when emitting outgoing webhooks.
    pub fn sign(integration: &Integration, body: &[u8]) -> String {
        hmac_sha256_hex(&integration.webhook_secret, body)
    }
}

/// Top-level façade.
pub struct BillingService {
    exporter: UsageExporter,
    engine: ChargebackEngine,
    relay: WebhookRelay,
}

impl BillingService {
    /// Construct the façade.
    pub fn new(
        exporter: UsageExporter,
        engine: ChargebackEngine,
        relay: WebhookRelay,
    ) -> Self {
        Self {
            exporter,
            engine,
            relay,
        }
    }

    /// Forward to the exporter.
    pub async fn export(
        &self,
        caller: &User,
        owner_id: Uuid,
    ) -> Result<Vec<UsageMeter>, BillingError> {
        self.exporter.export(caller, owner_id).await
    }

    /// Forward to the engine.
    pub async fn compute(
        &self,
        caller: &User,
        owner_id: Uuid,
        prices: &[(UsageUnit, u64)],
        currency: &str,
    ) -> Result<Chargeback, BillingError> {
        self.engine.compute(caller, owner_id, prices, currency).await
    }

    /// Forward to the relay.
    pub async fn verify(
        &self,
        caller: &User,
        integration_id: Uuid,
        body: &[u8],
        presented_signature: Option<&str>,
    ) -> Result<RelayOutcome, BillingError> {
        self.relay
            .verify(caller, integration_id, body, presented_signature)
            .await
    }
}

fn require_admin(caller: &User) -> Result<(), BillingError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(BillingError::Forbidden),
    }
}
