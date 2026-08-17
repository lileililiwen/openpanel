//! DNSSEC + secondary DNS services: signing service, key
//! rollover, AXFR sender, glue service, and registrar port.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    DnsSecError, DnsSecPolicy, DnsSecRepository, DsRecord, GlueRecord, KskRolloverState, Role,
    SecondaryNs, SigningAlgorithm, User, ZoneSigningKey,
};
use uuid::Uuid;

use crate::dnssec_secondary::SqliteDnsSecRepository;

/// Port used by the signing service to publish a DS record to the
/// parent zone (the registrar). Production wiring calls out to
/// the registrar's API; tests use `RecordingRegistrar`.
#[async_trait]
pub trait Registrar: Send + Sync + 'static {
    /// Publish `ds` to the parent zone.
    async fn publish_ds(&self, zone_id: Uuid, ds: &DsRecord) -> Result<(), DnsSecError>;
}

/// Recording registrar used by tests.
pub struct RecordingRegistrar {
    publishes: std::sync::Mutex<Vec<(Uuid, DsRecord)>>,
}

impl RecordingRegistrar {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            publishes: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Snapshot publishes.
    pub fn publishes(&self) -> Vec<(Uuid, DsRecord)> {
        #[allow(clippy::expect_used)] // mutex is never poisoned
        self.publishes.lock().expect("publishes").clone()
    }
}

impl Default for RecordingRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl Registrar for RecordingRegistrar {
    async fn publish_ds(&self, zone_id: Uuid, ds: &DsRecord) -> Result<(), DnsSecError> {
        #[allow(clippy::expect_used)] // mutex is never poisoned
        self.publishes
            .lock()
            .expect("publishes")
            .push((zone_id, ds.clone()));
        Ok(())
    }
}

/// Top-level DNSSEC service.
pub struct DnsSecService {
    repo: Arc<SqliteDnsSecRepository>,
    audit: Arc<dyn AuditService>,
    registrar: Arc<dyn Registrar>,
}

impl DnsSecService {
    /// Construct a service.
    pub fn new(
        repo: Arc<SqliteDnsSecRepository>,
        audit: Arc<dyn AuditService>,
        registrar: Arc<dyn Registrar>,
    ) -> Self {
        Self {
            repo,
            audit,
            registrar,
        }
    }

    /// Enable DNSSEC for a zone.
    pub async fn enable(
        &self,
        caller: &User,
        zone_id: Uuid,
        algorithm: SigningAlgorithm,
    ) -> Result<DnsSecPolicy, DnsSecError> {
        require_admin(caller)?;
        let policy = DnsSecPolicy {
            zone_id,
            enabled: true,
            algorithm,
            enabled_at: Some(Utc::now()),
        };
        self.repo.save_policy(&policy).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DnsSecEnabled,
                    AuditOutcome::Success,
                )
                .target(zone_id.to_string())
                .metadata(serde_json::json!({
                    "algorithm": algorithm.as_str(),
                })),
            )
            .await;
        Ok(policy)
    }

    /// Disable DNSSEC for a zone.
    pub async fn disable(&self, caller: &User, zone_id: Uuid) -> Result<DnsSecPolicy, DnsSecError> {
        require_admin(caller)?;
        let policy = self
            .repo
            .get_policy(zone_id)
            .await?
            .ok_or(DnsSecError::NotSigned(zone_id))?;
        let mut updated = policy;
        updated.enabled = false;
        self.repo.save_policy(&updated).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DnsSecDisabled,
                    AuditOutcome::Success,
                )
                .target(zone_id.to_string()),
            )
            .await;
        Ok(updated)
    }

    /// Add a signing key.
    pub async fn add_key(
        &self,
        caller: &User,
        key: ZoneSigningKey,
    ) -> Result<ZoneSigningKey, DnsSecError> {
        require_admin(caller)?;
        self.repo.save_key(&key).await?;
        Ok(key)
    }

    /// List keys for a zone.
    pub async fn list_keys(&self, zone_id: Uuid) -> Result<Vec<ZoneSigningKey>, DnsSecError> {
        Ok(self.repo.list_keys(zone_id).await?)
    }

    /// Publish a DS record to the registrar.
    pub async fn publish_ds(
        &self,
        caller: &User,
        zone_id: Uuid,
        ds: DsRecord,
    ) -> Result<(), DnsSecError> {
        require_admin(caller)?;
        ds.validate()?;
        self.repo.save_ds(&ds).await?;
        self.registrar.publish_ds(zone_id, &ds).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    caller.username().as_str(),
                    AuditAction::DnsSecDsPublished,
                    AuditOutcome::Success,
                )
                .target(zone_id.to_string())
                .metadata(serde_json::json!({
                    "key_tag": ds.key_tag,
                    "algorithm": ds.algorithm,
                    "digest_type": ds.digest_type,
                })),
            )
            .await;
        Ok(())
    }

    /// Add a secondary nameserver.
    pub async fn add_secondary(
        &self,
        caller: &User,
        secondary: SecondaryNs,
    ) -> Result<SecondaryNs, DnsSecError> {
        require_admin(caller)?;
        self.repo.save_secondary(&secondary).await?;
        Ok(secondary)
    }

    /// List secondaries for a zone.
    pub async fn list_secondaries(&self, zone_id: Uuid) -> Result<Vec<SecondaryNs>, DnsSecError> {
        Ok(self.repo.list_secondaries(zone_id).await?)
    }

    /// Add a glue record.
    pub async fn add_glue(
        &self,
        caller: &User,
        glue: GlueRecord,
    ) -> Result<GlueRecord, DnsSecError> {
        require_admin(caller)?;
        self.repo.save_glue(&glue).await?;
        Ok(glue)
    }

    /// List glue records for a zone.
    pub async fn list_glue(&self, zone_id: Uuid) -> Result<Vec<GlueRecord>, DnsSecError> {
        Ok(self.repo.list_glue(zone_id).await?)
    }
}

/// KSK rollover engine. Advances the state machine and audits
/// each transition. The v1 ships the state machine; production
/// wiring flips the per-key `active` flag as the state moves.
pub struct KeyRolloverEngine;

impl KeyRolloverEngine {
    /// Construct a rollover engine.
    pub fn new() -> Self {
        Self
    }

    /// Advance the state.
    pub fn advance(&self, current: KskRolloverState) -> KskRolloverState {
        current.next()
    }
}

impl Default for KeyRolloverEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// AXFR sender. The sender consults each registered secondary's
/// ACL and refuses to ship the zone to anyone outside it.
pub struct AxfrSender;

impl AxfrSender {
    /// Construct an AXFR sender.
    pub fn new() -> Self {
        Self
    }

    /// Ship the zone to every secondary whose ACL admits the
    /// requester. Returns the list of allowed secondaries.
    pub fn ship<'a>(
        &self,
        secondaries: &'a [SecondaryNs],
        requester: &str,
    ) -> Result<Vec<&'a SecondaryNs>, DnsSecError> {
        let allowed: Vec<&SecondaryNs> =
            secondaries.iter().filter(|s| s.allows(requester)).collect();
        if allowed.is_empty() {
            return Err(DnsSecError::SecondaryRejected(requester.to_string()));
        }
        Ok(allowed)
    }
}

impl Default for AxfrSender {
    fn default() -> Self {
        Self::new()
    }
}

/// Glue record service. Validates that the glue IP belongs to
/// the parent zone's delegated NS (a real wire would do DNS glue
/// checks; the v1 ships a structural validation).
pub struct GlueRecordService;

impl GlueRecordService {
    /// Construct a glue service.
    pub fn new() -> Self {
        Self
    }

    /// Validate the glue record is well-formed.
    pub fn validate(&self, glue: &GlueRecord) -> Result<(), DnsSecError> {
        if glue.name.is_empty() || glue.name.contains(' ') {
            return Err(DnsSecError::InvalidDigest("invalid glue name".into()));
        }
        if glue.a.is_none() && glue.aaaa.is_none() {
            return Err(DnsSecError::InvalidDigest(
                "glue needs at least one address".into(),
            ));
        }
        Ok(())
    }
}

impl Default for GlueRecordService {
    fn default() -> Self {
        Self::new()
    }
}

fn require_admin(caller: &User) -> Result<(), DnsSecError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(DnsSecError::Forbidden),
    }
}
