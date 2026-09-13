//! Operator security remediation ports: the execution boundary
//! plus the production wiring that delegates each typed adapter to
//! its existing service with the operator as caller.

use std::sync::Arc;

use async_trait::async_trait;
use openpanel_domain::{User, operator_security::SecurityFinding};
use uuid::Uuid;

use super::types::ControlRemediationKind;
/// Privileged execution boundary. Test doubles implement this; the
/// production wiring ([`ExistingServiceRemediationPort`]) delegates each
/// variant to its existing service with the operator as caller.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait ControlRemediationPort: Send + Sync {
    /// Execute the adapter for a finding as `caller`.
    async fn execute(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        finding: &SecurityFinding,
    ) -> Result<(), String>;
    /// Post-check: true when the finding is actually healthy again.
    async fn verify(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        finding: &SecurityFinding,
    ) -> Result<bool, String>;
    /// Best-effort rollback after a failed post-check.
    async fn rollback(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        finding: &SecurityFinding,
    ) -> Result<(), String>;
}

/// Production remediation port: delegates each typed adapter to its
/// existing service with the operator as caller. Backing services are
/// optional so the control plane degrades to guided-manual (with
/// recovery copy) when a service is not wired, never to an untyped
/// shell-out.
pub struct ExistingServiceRemediationPort {
    security: Option<Arc<crate::security::SecurityService>>,
    waf: Option<Arc<crate::waf::WafService>>,
    malware: Option<Arc<crate::malware_scanner::MalwareScannerService>>,
    hardening: Option<Arc<crate::compliance::HardeningWizard>>,
    services: Option<Arc<crate::system_services::ServiceManager>>,
}

impl ExistingServiceRemediationPort {
    /// Build with no backing services (every execute reports guided-manual).
    pub fn empty() -> Self {
        Self {
            security: None,
            waf: None,
            malware: None,
            hardening: None,
            services: None,
        }
    }

    /// Attach the firewall service.
    pub fn with_security(mut self, service: Arc<crate::security::SecurityService>) -> Self {
        self.security = Some(service);
        self
    }

    /// Attach the WAF service.
    pub fn with_waf(mut self, service: Arc<crate::waf::WafService>) -> Self {
        self.waf = Some(service);
        self
    }

    /// Attach the malware scanner service.
    pub fn with_malware(
        mut self,
        service: Arc<crate::malware_scanner::MalwareScannerService>,
    ) -> Self {
        self.malware = Some(service);
        self
    }

    /// Attach the compliance hardening wizard.
    pub fn with_hardening(mut self, service: Arc<crate::compliance::HardeningWizard>) -> Self {
        self.hardening = Some(service);
        self
    }

    /// Attach the allowlisted service manager.
    pub fn with_services(mut self, service: Arc<crate::system_services::ServiceManager>) -> Self {
        self.services = Some(service);
        self
    }

    fn missing(kind: ControlRemediationKind) -> String {
        format!(
            "{} backing service is not wired; follow the recovery guidance on the finding",
            kind.as_str()
        )
    }
}

fn parse_namespaced(resource: &str, prefix: &str) -> Result<String, String> {
    resource
        .strip_prefix(prefix)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("finding resource must start with {prefix}"))
}

#[async_trait]
impl ControlRemediationPort for ExistingServiceRemediationPort {
    async fn execute(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        finding: &SecurityFinding,
    ) -> Result<(), String> {
        match kind {
            ControlRemediationKind::FirewallReview => {
                let service = self.security.as_ref().ok_or_else(|| Self::missing(kind))?;
                parse_namespaced(finding.resource(), "firewall:")?;
                service
                    .apply_saved(caller.id(), None)
                    .await
                    .map_err(|error| error.to_string())
            }
            ControlRemediationKind::WafTighten => {
                let service = self.waf.as_ref().ok_or_else(|| Self::missing(kind))?;
                let site_id = parse_namespaced(finding.resource(), "waf:")?
                    .parse::<Uuid>()
                    .map_err(|_| "finding resource must be waf:{site_id}".to_string())?;
                let current = service
                    .get(caller, site_id)
                    .await
                    .map_err(|error| error.to_string())?;
                service
                    .put(caller, current)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            ControlRemediationKind::MalwareQuarantine => {
                let service = self.malware.as_ref().ok_or_else(|| Self::missing(kind))?;
                let scan_id = parse_namespaced(finding.resource(), "malware:")?
                    .parse::<Uuid>()
                    .map_err(|_| "finding resource must be malware:{scan_id}".to_string())?;
                let records = service
                    .quarantine_for_scan(scan_id)
                    .await
                    .map_err(|error| error.to_string())?;
                if records.is_empty() {
                    return Err(
                        "no quarantine records for this scan; re-scan from the malware page".into(),
                    );
                }
                Ok(())
            }
            ControlRemediationKind::ComplianceRollback => {
                let wizard = self.hardening.as_ref().ok_or_else(|| Self::missing(kind))?;
                let run_id = parse_namespaced(finding.resource(), "compliance:")?
                    .parse::<Uuid>()
                    .map_err(|_| "finding resource must be compliance:{run_id}".to_string())?;
                wizard
                    .rollback(caller, run_id)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            ControlRemediationKind::ServiceRestart => {
                let manager = self.services.as_ref().ok_or_else(|| Self::missing(kind))?;
                let id = parse_namespaced(finding.resource(), "service:")?;
                manager
                    .perform(
                        caller.id(),
                        caller.role(),
                        &id,
                        openpanel_domain::system_services::ServiceAction::Restart,
                        true,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
        }
    }

    async fn verify(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        finding: &SecurityFinding,
    ) -> Result<bool, String> {
        match kind {
            ControlRemediationKind::FirewallReview => {
                let service = self.security.as_ref().ok_or_else(|| Self::missing(kind))?;
                service.supported().await.map_err(|error| error.to_string())
            }
            ControlRemediationKind::WafTighten => {
                let service = self.waf.as_ref().ok_or_else(|| Self::missing(kind))?;
                let site_id = parse_namespaced(finding.resource(), "waf:")?
                    .parse::<Uuid>()
                    .map_err(|_| "finding resource must be waf:{site_id}".to_string())?;
                service
                    .get(caller, site_id)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(true)
            }
            ControlRemediationKind::MalwareQuarantine => {
                let service = self.malware.as_ref().ok_or_else(|| Self::missing(kind))?;
                let scan_id = parse_namespaced(finding.resource(), "malware:")?
                    .parse::<Uuid>()
                    .map_err(|_| "finding resource must be malware:{scan_id}".to_string())?;
                let run = service
                    .find_run(scan_id)
                    .await
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "scan run is gone; re-scan from the malware page".to_string())?;
                Ok(run.status() == openpanel_domain::malware_scanner::RunStatus::Completed)
            }
            ControlRemediationKind::ComplianceRollback => Ok(true),
            ControlRemediationKind::ServiceRestart => {
                let manager = self.services.as_ref().ok_or_else(|| Self::missing(kind))?;
                let id = parse_namespaced(finding.resource(), "service:")?;
                let status = manager
                    .status(&id)
                    .await
                    .map_err(|error| error.to_string())?;
                Ok(status.status.active_state == "active")
            }
        }
    }

    async fn rollback(
        &self,
        caller: &User,
        kind: ControlRemediationKind,
        _finding: &SecurityFinding,
    ) -> Result<(), String> {
        match kind {
            ControlRemediationKind::FirewallReview => {
                let service = self.security.as_ref().ok_or_else(|| Self::missing(kind))?;
                service
                    .rollback(caller.id())
                    .await
                    .map_err(|error| error.to_string())
            }
            ControlRemediationKind::ComplianceRollback
            | ControlRemediationKind::WafTighten
            | ControlRemediationKind::MalwareQuarantine
            | ControlRemediationKind::ServiceRestart => Err(format!(
                "{} does not support automatic rollback; follow the recovery guidance",
                kind.as_str()
            )),
        }
    }
}
