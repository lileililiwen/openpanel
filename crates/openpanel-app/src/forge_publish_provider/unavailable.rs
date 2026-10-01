//! Fail-closed delivery adapters; real deployment is owned by its follow-up package.
use openpanel_domain::{
    ApplicationDeliveryError, ApplicationSource, BuildAdapter, Environment, Release, ReleaseDigest,
    ReleaseOutcome, RuntimeAdapter,
};
use uuid::Uuid;

/// Refuses builds until a production build adapter is composed.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableBuildAdapter;
impl BuildAdapter for UnavailableBuildAdapter {
    fn adapter_id(&self) -> &'static str {
        "unavailable-build-v1"
    }
    fn resolve_digest(
        &self,
        _application_id: Uuid,
        _source: &ApplicationSource,
    ) -> Result<ReleaseDigest, ApplicationDeliveryError> {
        Err(ApplicationDeliveryError::Invalid(
            "build adapter unavailable".into(),
        ))
    }
}
/// Refuses runtime mutations until a production runtime adapter is composed.
#[derive(Debug, Clone, Copy, Default)]
pub struct UnavailableRuntimeAdapter;
impl RuntimeAdapter for UnavailableRuntimeAdapter {
    fn adapter_id(&self) -> &'static str {
        "unavailable-runtime-v1"
    }
    fn deploy(
        &self,
        _release: &Release,
        _environment: &Environment,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Err(ApplicationDeliveryError::Invalid(
            "runtime adapter unavailable".into(),
        ))
    }
    fn rollback(
        &self,
        _release: &Release,
        _environment: &Environment,
        _target_release_id: Uuid,
    ) -> Result<ReleaseOutcome, ApplicationDeliveryError> {
        Err(ApplicationDeliveryError::Invalid(
            "runtime adapter unavailable".into(),
        ))
    }
}
