//! Transactional firewall and durable login-throttling orchestration.

use std::{net::IpAddr, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::security::{
    BreakGlassConfirmation, FirewallPolicy, FirewallRule, LoginKey, LoginThrottlePolicy,
    NetworkCidr, SecurityError, TemporaryBlock, trusted_client_ip,
};
use serde::Serialize;
use thiserror::Error;
use uuid::Uuid;

/// Privileged firewall boundary. Implementations must touch only `inet openpanel`.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait FirewallPort: Send + Sync {
    /// Whether nftables management is supported.
    async fn supported(&self) -> Result<bool, SecurityServiceError>;
    /// Syntax-check a complete candidate without applying it.
    async fn check(&self, candidate: &str) -> Result<bool, SecurityServiceError>;
    /// Atomically apply a checked candidate and preserve last-known-good state.
    async fn apply(&self, candidate: &str) -> Result<(), SecurityServiceError>;
    /// Verify protected access after application.
    async fn verify(&self) -> Result<bool, SecurityServiceError>;
    /// Restore last-known-good OpenPanel rules.
    async fn rollback(&self) -> Result<(), SecurityServiceError>;
}

/// Persistence boundary for rule state and login abuse records.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait SecurityRepository: Send + Sync {
    /// List managed rules.
    async fn list_rules(&self) -> Result<Vec<FirewallRule>, SecurityServiceError>;
    /// Fetch one managed rule.
    async fn get_rule(&self, id: Uuid) -> Result<Option<FirewallRule>, SecurityServiceError>;
    /// Insert or replace one validated rule.
    async fn save_rule(&self, rule: &FirewallRule) -> Result<(), SecurityServiceError>;
    /// Delete one rule.
    async fn delete_rule(&self, id: Uuid) -> Result<(), SecurityServiceError>;
    /// Replace the durable managed rule set after successful verification.
    async fn replace_rules(&self, rules: &[FirewallRule]) -> Result<(), SecurityServiceError>;
    /// Persist one generic failed-attempt observation.
    async fn save_failure(
        &self,
        key: &LoginKey,
        at: DateTime<Utc>,
    ) -> Result<(), SecurityServiceError>;
    /// Count recent failures for a key.
    async fn failure_count(
        &self,
        key: &LoginKey,
        since: DateTime<Utc>,
    ) -> Result<u32, SecurityServiceError>;
    /// Save or extend a temporary block.
    async fn save_block(&self, block: &TemporaryBlock) -> Result<(), SecurityServiceError>;
    /// Reduce account penalties after successful authentication.
    async fn clear_failures(&self, key: &LoginKey) -> Result<(), SecurityServiceError>;
    /// List active and recent blocks.
    async fn list_blocks(&self) -> Result<Vec<TemporaryBlock>, SecurityServiceError>;
    /// End a block by durable key.
    async fn unblock(&self, key: &LoginKey, at: DateTime<Utc>) -> Result<(), SecurityServiceError>;
    /// Fetch a durable block for pre-authentication enforcement.
    async fn active_block(
        &self,
        key: &LoginKey,
        at: DateTime<Utc>,
    ) -> Result<Option<TemporaryBlock>, SecurityServiceError>;
    /// List address networks exempted by the Owner.
    async fn list_allowlists(&self) -> Result<Vec<NetworkCidr>, SecurityServiceError>;
    /// Add one canonical exempt network.
    async fn add_allowlist(&self, network: NetworkCidr) -> Result<(), SecurityServiceError>;
    /// Remove one canonical exempt network.
    async fn remove_allowlist(&self, network: NetworkCidr) -> Result<(), SecurityServiceError>;
}

/// Injectable time source for repeatable throttling and watchdog tests.
pub trait Clock: Send + Sync {
    /// Current UTC instant.
    fn now(&self) -> DateTime<Utc>;
}

/// Fixed clock used by deterministic service tests.
pub struct FixedClock(DateTime<Utc>);
impl FixedClock {
    /// Construct at one instant.
    pub fn new(now: DateTime<Utc>) -> Self {
        Self(now)
    }
}
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

/// System UTC clock.
pub struct SystemClock;
impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Redacted application failures.
#[derive(Debug, Error)]
pub enum SecurityServiceError {
    /// Requested rule or block is absent.
    #[error("security record not found")]
    NotFound,
    /// Domain safety rejected the request.
    #[error("security validation failed: {0}")]
    Validation(String),
    /// `nft --check` rejected the candidate.
    #[error("firewall candidate syntax was rejected")]
    SyntaxRejected,
    /// Post-apply protected-access verification failed and rollback ran.
    #[error("firewall verification failed; last-known-good rules restored")]
    VerificationFailed,
    /// Privileged firewall adapter failed.
    #[error("firewall adapter unavailable")]
    Firewall,
    /// Durable security state failed.
    #[error("security persistence unavailable")]
    Persistence,
}

impl From<SecurityError> for SecurityServiceError {
    fn from(error: SecurityError) -> Self {
        Self::Validation(error.to_string())
    }
}

/// Transactional firewall use cases.
pub struct SecurityService {
    firewall: Arc<dyn FirewallPort>,
    repository: Arc<dyn SecurityRepository>,
    audit: Arc<dyn AuditService>,
    policy: FirewallPolicy,
    watchdog_timeout: std::time::Duration,
}

impl SecurityService {
    /// Compose the service with explicit privileged and persistence ports.
    pub fn with_ports(
        firewall: Arc<dyn FirewallPort>,
        repository: Arc<dyn SecurityRepository>,
        audit: Arc<dyn AuditService>,
        policy: FirewallPolicy,
    ) -> Self {
        Self {
            firewall,
            repository,
            audit,
            policy,
            watchdog_timeout: std::time::Duration::from_secs(30),
        }
    }

    /// Override the reachability watchdog deadline.
    pub fn with_watchdog_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.watchdog_timeout = timeout;
        self
    }

    /// Render a complete isolated-table preview.
    pub fn preview(
        &self,
        rules: &[FirewallRule],
        confirmation: Option<&BreakGlassConfirmation>,
    ) -> Result<String, SecurityServiceError> {
        self.policy.validate_candidate(rules, confirmation)?;
        Ok(render_ruleset(rules))
    }

    /// Report firewall support without mutating the host.
    pub async fn supported(&self) -> Result<bool, SecurityServiceError> {
        self.firewall.supported().await
    }

    /// List durable managed rules.
    pub async fn rules(&self) -> Result<Vec<FirewallRule>, SecurityServiceError> {
        self.repository.list_rules().await
    }

    /// Save one validated draft rule.
    pub async fn save_rule(
        &self,
        rule: FirewallRule,
    ) -> Result<FirewallRule, SecurityServiceError> {
        self.repository.save_rule(&rule).await?;
        Ok(rule)
    }

    /// Fetch one durable rule.
    pub async fn rule(&self, id: Uuid) -> Result<FirewallRule, SecurityServiceError> {
        self.repository
            .get_rule(id)
            .await?
            .ok_or(SecurityServiceError::NotFound)
    }

    /// Enable or disable a draft rule.
    pub async fn set_rule_enabled(
        &self,
        id: Uuid,
        enabled: bool,
    ) -> Result<FirewallRule, SecurityServiceError> {
        let mut rule = self.rule(id).await?;
        rule.set_enabled(enabled);
        self.repository.save_rule(&rule).await?;
        Ok(rule)
    }

    /// Delete a draft rule.
    pub async fn delete_rule(&self, id: Uuid) -> Result<(), SecurityServiceError> {
        if self.repository.get_rule(id).await?.is_none() {
            return Err(SecurityServiceError::NotFound);
        }
        self.repository.delete_rule(id).await
    }

    /// Preview all saved rules.
    pub async fn preview_saved(
        &self,
        confirmation: Option<&BreakGlassConfirmation>,
    ) -> Result<String, SecurityServiceError> {
        self.preview(&self.repository.list_rules().await?, confirmation)
    }

    /// Apply all saved rules transactionally.
    pub async fn apply_saved(
        &self,
        actor: Uuid,
        confirmation: Option<&BreakGlassConfirmation>,
    ) -> Result<(), SecurityServiceError> {
        self.apply_candidate(actor, self.repository.list_rules().await?, confirmation)
            .await
    }

    /// List temporary login blocks.
    pub async fn blocks(&self) -> Result<Vec<TemporaryBlock>, SecurityServiceError> {
        self.repository.list_blocks().await
    }

    /// End a temporary block immediately and audit it.
    pub async fn unblock(&self, actor: Uuid, key: LoginKey) -> Result<(), SecurityServiceError> {
        self.repository.unblock(&key, Utc::now()).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::SecurityBlockChanged,
                    AuditOutcome::Success,
                )
                .target(key.storage_key()),
            )
            .await;
        Ok(())
    }

    /// List login address allowlists.
    pub async fn allowlists(&self) -> Result<Vec<NetworkCidr>, SecurityServiceError> {
        self.repository.list_allowlists().await
    }

    /// Add a login address allowlist and audit the canonical network.
    pub async fn add_allowlist(
        &self,
        actor: Uuid,
        network: NetworkCidr,
    ) -> Result<NetworkCidr, SecurityServiceError> {
        self.repository.add_allowlist(network).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::SecurityBlockChanged,
                    AuditOutcome::Success,
                )
                .target(network.to_string()),
            )
            .await;
        Ok(network)
    }

    /// Delete a login address allowlist.
    pub async fn remove_allowlist(
        &self,
        actor: Uuid,
        network: NetworkCidr,
    ) -> Result<(), SecurityServiceError> {
        self.repository.remove_allowlist(network).await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::SecurityBlockChanged,
                    AuditOutcome::Success,
                )
                .target(network.to_string()),
            )
            .await;
        Ok(())
    }

    /// Check, apply, verify, persist, audit, and rollback on verification failure.
    pub async fn apply_candidate(
        &self,
        actor: Uuid,
        rules: Vec<FirewallRule>,
        confirmation: Option<&BreakGlassConfirmation>,
    ) -> Result<(), SecurityServiceError> {
        let candidate = self.preview(&rules, confirmation)?;
        if !self.firewall.check(&candidate).await? {
            return Err(SecurityServiceError::SyntaxRejected);
        }
        self.firewall.apply(&candidate).await?;
        let verified = tokio::time::timeout(self.watchdog_timeout, self.firewall.verify()).await;
        if !matches!(verified, Ok(Ok(true))) {
            self.firewall.rollback().await?;
            return Err(SecurityServiceError::VerificationFailed);
        }
        self.repository.replace_rules(&rules).await?;
        let _ = self.audit.record(AuditEvent::new(actor.to_string(), AuditAction::FirewallChanged, AuditOutcome::Success).metadata(serde_json::json!({"rule_ids": rules.iter().map(|rule| rule.id()).collect::<Vec<_>>() }))).await;
        Ok(())
    }

    /// Explicitly restore last-known-good OpenPanel rules.
    pub async fn rollback(&self, actor: Uuid) -> Result<(), SecurityServiceError> {
        self.firewall.rollback().await?;
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.to_string(),
                    AuditAction::FirewallChanged,
                    AuditOutcome::Success,
                )
                .metadata(serde_json::json!({"operation":"rollback"})),
            )
            .await;
        Ok(())
    }
}

fn render_ruleset(rules: &[FirewallRule]) -> String {
    let mut candidate = String::from(
        "table inet openpanel {\n chain input { type filter hook input priority 0; policy accept;\n",
    );
    for rule in rules.iter().filter(|rule| rule.enabled()) {
        candidate.push_str("  ");
        candidate.push_str(&rule.render_nft());
        candidate.push('\n');
    }
    candidate.push_str(" }\n}\n");
    candidate
}

/// Generic public throttle decision, identical for known and unknown accounts.
#[derive(Debug, Clone, Serialize)]
pub struct LoginThrottleDecision {
    /// Stable generic response.
    pub public_message: &'static str,
    /// Delay clients may surface without account disclosure.
    pub retry_after_seconds: Option<u64>,
}

/// Durable dual-key login throttling service.
pub struct LoginThrottleService {
    repository: Arc<dyn SecurityRepository>,
    clock: Arc<dyn Clock>,
    policy: LoginThrottlePolicy,
    trusted_proxies: Vec<NetworkCidr>,
}

impl LoginThrottleService {
    /// Construct with explicit durable repository, clock, policy, and trusted proxies.
    pub fn new(
        repository: Arc<dyn SecurityRepository>,
        clock: Arc<dyn Clock>,
        policy: LoginThrottlePolicy,
        trusted_proxies: Vec<NetworkCidr>,
    ) -> Self {
        Self {
            repository,
            clock,
            policy,
            trusted_proxies,
        }
    }

    /// Record a failed password attempt against both normalized account and trusted address keys.
    pub async fn record_failure(
        &self,
        account: &str,
        peer: IpAddr,
        forwarded: Option<IpAddr>,
    ) -> Result<LoginThrottleDecision, SecurityServiceError> {
        let now = self.clock.now();
        let address = trusted_client_ip(peer, forwarded, &self.trusted_proxies);
        if self
            .repository
            .list_allowlists()
            .await?
            .iter()
            .any(|network| network.contains(address))
        {
            return Ok(LoginThrottleDecision {
                public_message: "invalid credentials",
                retry_after_seconds: None,
            });
        }
        let keys = [LoginKey::account(account)?, LoginKey::ip(address)];
        let since = now
            - chrono::Duration::from_std(self.policy.window())
                .map_err(|_| SecurityServiceError::Validation("invalid throttle window".into()))?;
        let mut retry = None;
        for key in &keys {
            self.repository.save_failure(key, now).await?;
            let failures = self.repository.failure_count(key, since).await?;
            if failures >= self.policy.attempts() {
                let duration = self
                    .policy
                    .block_duration(failures - self.policy.attempts() + 1);
                let block = TemporaryBlock::new(key.clone(), now, duration)?;
                self.repository.save_block(&block).await?;
                retry = Some(retry.unwrap_or(0).max(duration.as_secs()));
            }
        }
        Ok(LoginThrottleDecision {
            public_message: "invalid credentials",
            retry_after_seconds: retry,
        })
    }

    /// Return generic rate-limit metadata when either normalized key is blocked.
    pub async fn check(
        &self,
        account: &str,
        peer: IpAddr,
        forwarded: Option<IpAddr>,
    ) -> Result<Option<LoginThrottleDecision>, SecurityServiceError> {
        let now = self.clock.now();
        let address = trusted_client_ip(peer, forwarded, &self.trusted_proxies);
        if self
            .repository
            .list_allowlists()
            .await?
            .iter()
            .any(|network| network.contains(address))
        {
            return Ok(None);
        }
        for key in [LoginKey::account(account)?, LoginKey::ip(address)] {
            if let Some(block) = self.repository.active_block(&key, now).await?
                && block.is_active(now)
            {
                let seconds = (block.expires_at() - now)
                    .to_std()
                    .map_or(1, |duration| duration.as_secs().max(1));
                return Ok(Some(LoginThrottleDecision {
                    public_message: "invalid credentials",
                    retry_after_seconds: Some(seconds),
                }));
            }
        }
        Ok(None)
    }

    /// Clear normalized account failures after successful password verification.
    pub async fn record_success(&self, account: &str) -> Result<(), SecurityServiceError> {
        self.repository
            .clear_failures(&LoginKey::account(account)?)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::{TimeZone, Utc};
    use openpanel_domain::security::*;
    use openpanel_test_support::MockAudit;
    use uuid::Uuid;

    use super::*;

    fn allow_https() -> FirewallRule {
        FirewallRule::new(
            Uuid::new_v4(),
            Protocol::Tcp,
            PortRange::new(443, 443).unwrap(),
            NetworkCidr::parse("0.0.0.0/0").unwrap(),
            RuleAction::Allow,
            "HTTPS",
            true,
        )
        .unwrap()
    }

    #[test]
    fn preview_omits_disabled_rules() {
        let firewall = Arc::new(MockFirewallPort::new());
        let service = SecurityService::with_ports(
            firewall,
            Arc::new(MockSecurityRepository::new()),
            Arc::new(MockAudit::stub()),
            FirewallPolicy::new(vec![22, 8443]).unwrap(),
        );
        let mut disabled = allow_https();
        disabled.set_enabled(false);
        assert!(
            !service
                .preview(&[disabled], None)
                .unwrap()
                .contains("dport 443")
        );
    }

    #[tokio::test]
    async fn syntax_check_failure_never_applies_or_rolls_back_active_rules() {
        let mut firewall = MockFirewallPort::new();
        firewall.expect_check().once().returning(|_| Ok(false));
        firewall.expect_apply().never();
        firewall.expect_verify().never();
        firewall.expect_rollback().never();
        let service = SecurityService::with_ports(
            Arc::new(firewall),
            Arc::new(MockSecurityRepository::new()),
            Arc::new(MockAudit::stub()),
            FirewallPolicy::new(vec![22, 8443]).unwrap(),
        );

        assert!(matches!(
            service
                .apply_candidate(Uuid::new_v4(), vec![allow_https()], None)
                .await,
            Err(SecurityServiceError::SyntaxRejected)
        ));
    }

    #[tokio::test]
    async fn failed_post_apply_verification_restores_last_known_good() {
        let mut firewall = MockFirewallPort::new();
        let mut sequence = mockall::Sequence::new();
        firewall
            .expect_check()
            .once()
            .in_sequence(&mut sequence)
            .returning(|_| Ok(true));
        firewall
            .expect_apply()
            .once()
            .in_sequence(&mut sequence)
            .returning(|_| Ok(()));
        firewall
            .expect_verify()
            .once()
            .in_sequence(&mut sequence)
            .returning(|| Ok(false));
        firewall
            .expect_rollback()
            .once()
            .in_sequence(&mut sequence)
            .returning(|| Ok(()));
        let service = SecurityService::with_ports(
            Arc::new(firewall),
            Arc::new(MockSecurityRepository::new()),
            Arc::new(MockAudit::stub()),
            FirewallPolicy::new(vec![22, 8443]).unwrap(),
        );

        assert!(matches!(
            service
                .apply_candidate(Uuid::new_v4(), vec![allow_https()], None)
                .await,
            Err(SecurityServiceError::VerificationFailed)
        ));
    }

    #[tokio::test]
    async fn watchdog_timeout_restores_last_known_good() {
        struct SlowFirewall(std::sync::atomic::AtomicBool);
        #[async_trait]
        impl FirewallPort for SlowFirewall {
            async fn supported(&self) -> Result<bool, SecurityServiceError> {
                Ok(true)
            }

            async fn check(&self, _: &str) -> Result<bool, SecurityServiceError> {
                Ok(true)
            }

            async fn apply(&self, _: &str) -> Result<(), SecurityServiceError> {
                Ok(())
            }

            async fn verify(&self) -> Result<bool, SecurityServiceError> {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                Ok(true)
            }

            async fn rollback(&self) -> Result<(), SecurityServiceError> {
                self.0.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }
        let firewall = Arc::new(SlowFirewall(std::sync::atomic::AtomicBool::new(false)));
        let service = SecurityService::with_ports(
            firewall.clone(),
            Arc::new(MockSecurityRepository::new()),
            Arc::new(MockAudit::stub()),
            FirewallPolicy::new(vec![22, 8443]).unwrap(),
        )
        .with_watchdog_timeout(std::time::Duration::from_millis(5));
        assert!(matches!(
            service
                .apply_candidate(Uuid::new_v4(), vec![allow_https()], None)
                .await,
            Err(SecurityServiceError::VerificationFailed)
        ));
        assert!(firewall.0.load(std::sync::atomic::Ordering::SeqCst));
    }

    #[tokio::test]
    async fn repeated_failures_create_generic_bounded_account_and_ip_blocks() {
        let mut repository = MockSecurityRepository::new();
        repository
            .expect_failure_count()
            .times(2)
            .returning(|_, _| Ok(5));
        repository
            .expect_save_failure()
            .times(2)
            .returning(|_, _| Ok(()));
        repository
            .expect_save_block()
            .times(2)
            .returning(|_| Ok(()));
        repository
            .expect_list_allowlists()
            .once()
            .returning(|| Ok(vec![]));
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 2, 0, 0).unwrap();
        let service = LoginThrottleService::new(
            Arc::new(repository),
            Arc::new(FixedClock::new(now)),
            LoginThrottlePolicy::new(
                5,
                std::time::Duration::from_secs(300),
                std::time::Duration::from_secs(30),
                std::time::Duration::from_secs(3600),
            )
            .unwrap(),
            vec![NetworkCidr::parse("10.0.0.0/8").unwrap()],
        );
        let result = service
            .record_failure(
                " Alice ",
                "10.1.1.1".parse().unwrap(),
                Some("198.51.100.9".parse().unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(result.public_message, "invalid credentials");
        assert!(result.retry_after_seconds.is_some());
    }

    #[tokio::test]
    async fn active_account_or_trusted_ip_block_prevents_authentication() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 2, 0, 0).unwrap();
        let mut repository = MockSecurityRepository::new();
        repository
            .expect_list_allowlists()
            .once()
            .returning(|| Ok(vec![]));
        repository
            .expect_active_block()
            .times(2)
            .returning(move |key, _| {
                if key.storage_key() == "ip:198.51.100.9" {
                    Ok(Some(
                        TemporaryBlock::new(key.clone(), now, std::time::Duration::from_secs(30))
                            .unwrap(),
                    ))
                } else {
                    Ok(None)
                }
            });
        let service = LoginThrottleService::new(
            Arc::new(repository),
            Arc::new(FixedClock::new(now)),
            LoginThrottlePolicy::new(
                5,
                std::time::Duration::from_secs(300),
                std::time::Duration::from_secs(30),
                std::time::Duration::from_secs(3600),
            )
            .unwrap(),
            vec![NetworkCidr::parse("10.0.0.0/8").unwrap()],
        );
        let decision = service
            .check(
                "alice",
                "10.1.1.1".parse().unwrap(),
                Some("198.51.100.9".parse().unwrap()),
            )
            .await
            .unwrap();
        assert_eq!(
            decision.and_then(|value| value.retry_after_seconds),
            Some(30)
        );
    }

    #[tokio::test]
    async fn allowlisted_trusted_address_bypasses_failure_storage() {
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 2, 0, 0).unwrap();
        let mut repository = MockSecurityRepository::new();
        repository
            .expect_list_allowlists()
            .once()
            .returning(|| Ok(vec![NetworkCidr::parse("198.51.100.0/24").unwrap()]));
        repository.expect_save_failure().never();
        let service = LoginThrottleService::new(
            Arc::new(repository),
            Arc::new(FixedClock::new(now)),
            LoginThrottlePolicy::new(
                5,
                std::time::Duration::from_secs(300),
                std::time::Duration::from_secs(30),
                std::time::Duration::from_secs(3600),
            )
            .unwrap(),
            vec![],
        );
        let decision = service
            .record_failure("alice", "198.51.100.9".parse().unwrap(), None)
            .await
            .unwrap();
        assert_eq!(decision.retry_after_seconds, None);
    }
}
