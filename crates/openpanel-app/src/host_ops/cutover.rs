#![allow(missing_docs)]
//! Reversible cutover for DNS, mail, and SSL.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use openpanel_domain::AgentError;

/// Evidence for one cutover action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CutoverEvidence {
    /// Stable cutover id.
    pub cutover_id: Uuid,
    /// Kind of cutover (dns/mail/ssl).
    pub kind: String,
    /// Whether the preflight passed.
    pub preflight_passed: bool,
    /// Rollback window.
    pub rollback_ttl: Duration,
    /// Human-readable diagnostic (redacted).
    pub diagnostic: String,
}

#[async_trait]
pub trait DnsCutover: Send + Sync {
    async fn preflight(&self) -> Result<bool, AgentError>;
    async fn commit(&self) -> Result<CutoverEvidence, AgentError>;
    async fn rollback(&self) -> Result<CutoverEvidence, AgentError>;
}

#[async_trait]
pub trait MailCutover: Send + Sync {
    async fn preflight(&self) -> Result<bool, AgentError>;
    async fn commit(&self) -> Result<CutoverEvidence, AgentError>;
    async fn rollback(&self) -> Result<CutoverEvidence, AgentError>;
}

#[async_trait]
pub trait SslCutover: Send + Sync {
    async fn preflight(&self) -> Result<bool, AgentError>;
    async fn commit(&self) -> Result<CutoverEvidence, AgentError>;
    async fn rollback(&self) -> Result<CutoverEvidence, AgentError>;
}

pub struct CutoverService<D, M, S> {
    dns: D,
    mail: M,
    ssl: S,
    ttl: Duration,
}

impl<D, M, S> CutoverService<D, M, S>
where
    D: DnsCutover,
    M: MailCutover,
    S: SslCutover,
{
    pub fn new(dns: D, mail: M, ssl: S) -> Self {
        Self {
            dns,
            mail,
            ssl,
            ttl: Duration::hours(24),
        }
    }

    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub async fn dns_preflight(&self) -> Result<bool, AgentError> {
        self.dns.preflight().await
    }

    pub async fn dns_commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !self.dns.preflight().await? {
            return Err(AgentError::InvalidManifest(
                "dns preflight failed — cutover refused".to_string(),
            ));
        }
        self.dns.commit().await
    }

    pub async fn dns_rollback(
        &self,
        committed_at: DateTime<Utc>,
    ) -> Result<CutoverEvidence, AgentError> {
        if Utc::now() > committed_at + self.ttl {
            return Err(AgentError::InvalidManifest(
                "dns rollback window expired".to_string(),
            ));
        }
        self.dns.rollback().await
    }

    pub async fn mail_preflight(&self) -> Result<bool, AgentError> {
        self.mail.preflight().await
    }

    pub async fn mail_commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !self.mail.preflight().await? {
            return Err(AgentError::InvalidManifest(
                "mail preflight failed — cutover refused".to_string(),
            ));
        }
        self.mail.commit().await
    }

    pub async fn mail_rollback(
        &self,
        committed_at: DateTime<Utc>,
    ) -> Result<CutoverEvidence, AgentError> {
        if Utc::now() > committed_at + self.ttl {
            return Err(AgentError::InvalidManifest(
                "mail rollback window expired".to_string(),
            ));
        }
        self.mail.rollback().await
    }

    pub async fn ssl_preflight(&self) -> Result<bool, AgentError> {
        self.ssl.preflight().await
    }

    pub async fn ssl_commit(&self) -> Result<CutoverEvidence, AgentError> {
        if !self.ssl.preflight().await? {
            return Err(AgentError::InvalidManifest(
                "ssl preflight failed — cutover refused".to_string(),
            ));
        }
        self.ssl.commit().await
    }

    pub async fn ssl_rollback(
        &self,
        committed_at: DateTime<Utc>,
    ) -> Result<CutoverEvidence, AgentError> {
        if Utc::now() > committed_at + self.ttl {
            return Err(AgentError::InvalidManifest(
                "ssl rollback window expired".to_string(),
            ));
        }
        self.ssl.rollback().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PassCutover;
    struct FailCutover;

    #[async_trait]
    impl DnsCutover for PassCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(true)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "dns".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "dns committed".to_string(),
            })
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "dns".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "dns rolled back".to_string(),
            })
        }
    }
    #[async_trait]
    impl MailCutover for PassCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(true)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "mail".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "mail committed".to_string(),
            })
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "mail".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "mail rolled back".to_string(),
            })
        }
    }
    #[async_trait]
    impl SslCutover for PassCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(true)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "ssl".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "ssl committed".to_string(),
            })
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            Ok(CutoverEvidence {
                cutover_id: Uuid::new_v4(),
                kind: "ssl".to_string(),
                preflight_passed: true,
                rollback_ttl: Duration::hours(24),
                diagnostic: "ssl rolled back".to_string(),
            })
        }
    }
    #[async_trait]
    impl DnsCutover for FailCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(false)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
    }
    #[async_trait]
    impl MailCutover for FailCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(false)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
    }
    #[async_trait]
    impl SslCutover for FailCutover {
        async fn preflight(&self) -> Result<bool, AgentError> {
            Ok(false)
        }
        async fn commit(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
        async fn rollback(&self) -> Result<CutoverEvidence, AgentError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn dns_cutover_preflight_required() {
        let svc = CutoverService::new(FailCutover, PassCutover, PassCutover);
        assert!(!svc.dns_preflight().await.unwrap());
        let err = svc.dns_commit().await.expect_err("preflight failed");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
        let svc2 = CutoverService::new(PassCutover, PassCutover, PassCutover);
        assert!(svc2.dns_commit().await.is_ok());
    }

    #[tokio::test]
    async fn mail_cutover_rollback_within_ttl() {
        let svc = CutoverService::new(PassCutover, PassCutover, PassCutover);
        let committed_at = Utc::now();
        assert!(svc.mail_rollback(committed_at).await.is_ok());
        let expired = Utc::now() - Duration::hours(25);
        let err = svc.mail_rollback(expired).await.expect_err("ttl expired");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
    }

    #[tokio::test]
    async fn ssl_cutover_refused_when_preflight_fails() {
        let svc = CutoverService::new(PassCutover, PassCutover, FailCutover);
        let err = svc.ssl_commit().await.expect_err("preflight failed");
        assert!(matches!(err, AgentError::InvalidManifest(_)));
    }
}
