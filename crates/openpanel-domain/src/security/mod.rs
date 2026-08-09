//! Host firewall and login-abuse protection domain model.

use std::{fmt, net::IpAddr, time::Duration};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

/// Validation and lockout failures.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SecurityError {
    /// CIDR is malformed or its prefix is out of range.
    #[error("invalid network CIDR")]
    InvalidCidr,
    /// Port range is empty, reversed, or includes port zero.
    #[error("invalid port range")]
    InvalidPort,
    /// Rule fields are ambiguous or unsafe.
    #[error("invalid firewall rule")]
    InvalidRule,
    /// A firewall candidate could remove management access.
    #[error("firewall candidate risks host lockout")]
    LockoutRisk,
    /// Login key is empty or unsafe.
    #[error("invalid login throttle key")]
    InvalidLoginKey,
    /// Throttle configuration is inconsistent.
    #[error("invalid login throttle policy")]
    InvalidThrottlePolicy,
    /// Block duration is zero or invalid.
    #[error("invalid temporary block")]
    InvalidBlock,
}

/// Canonical IPv4 or IPv6 network and prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct NetworkCidr {
    network: IpAddr,
    prefix: u8,
}

impl NetworkCidr {
    /// Parse and canonicalize a CIDR.
    pub fn parse(value: &str) -> Result<Self, SecurityError> {
        let (address, prefix) = value.split_once('/').ok_or(SecurityError::InvalidCidr)?;
        let address: IpAddr = address.parse().map_err(|_| SecurityError::InvalidCidr)?;
        let prefix: u8 = prefix.parse().map_err(|_| SecurityError::InvalidCidr)?;
        let network = match address {
            IpAddr::V4(address) if prefix <= 32 => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - prefix)
                };
                IpAddr::V4(std::net::Ipv4Addr::from(u32::from(address) & mask))
            }
            IpAddr::V6(address) if prefix <= 128 => {
                let mask = if prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - prefix)
                };
                IpAddr::V6(std::net::Ipv6Addr::from(u128::from(address) & mask))
            }
            _ => return Err(SecurityError::InvalidCidr),
        };
        Ok(Self { network, prefix })
    }

    /// Whether this network contains an address of the same family.
    pub fn contains(&self, address: IpAddr) -> bool {
        match (self.network, address) {
            (IpAddr::V4(network), IpAddr::V4(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u32::MAX << (32 - self.prefix)
                };
                u32::from(network) == u32::from(address) & mask
            }
            (IpAddr::V6(network), IpAddr::V6(address)) => {
                let mask = if self.prefix == 0 {
                    0
                } else {
                    u128::MAX << (128 - self.prefix)
                };
                u128::from(network) == u128::from(address) & mask
            }
            _ => false,
        }
    }

    /// Whether this is the broadest network for its address family.
    pub fn is_all(&self) -> bool {
        self.prefix == 0
    }
}

impl fmt::Display for NetworkCidr {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}/{}", self.network, self.prefix)
    }
}

/// Inclusive transport port range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PortRange {
    start: u16,
    end: u16,
}
impl PortRange {
    /// Create a nonzero ordered range.
    pub fn new(start: u16, end: u16) -> Result<Self, SecurityError> {
        if start == 0 || end == 0 || start > end {
            return Err(SecurityError::InvalidPort);
        }
        Ok(Self { start, end })
    }

    /// Whether this range includes a port.
    pub fn contains(&self, port: u16) -> bool {
        (self.start..=self.end).contains(&port)
    }

    /// First port.
    pub fn start(&self) -> u16 {
        self.start
    }

    /// Last port.
    pub fn end(&self) -> u16 {
        self.end
    }
}
impl fmt::Display for PortRange {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.start == self.end {
            write!(formatter, "{}", self.start)
        } else {
            write!(formatter, "{}-{}", self.start, self.end)
        }
    }
}

/// Transport protocol selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protocol {
    /// TCP.
    Tcp,
    /// UDP.
    Udp,
    /// Ambiguous protocol, rejected for port rules.
    Any,
}
/// Firewall decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Accept matching traffic.
    Allow,
    /// Drop matching traffic.
    Deny,
}

/// Typed managed inbound rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FirewallRule {
    id: Uuid,
    protocol: Protocol,
    ports: PortRange,
    source: NetworkCidr,
    action: RuleAction,
    comment: String,
    enabled: bool,
}
impl FirewallRule {
    /// Construct a safe typed rule.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        protocol: Protocol,
        ports: PortRange,
        source: NetworkCidr,
        action: RuleAction,
        comment: impl Into<String>,
        enabled: bool,
    ) -> Result<Self, SecurityError> {
        let comment = comment.into();
        let lower = comment.to_ascii_lowercase();
        if protocol == Protocol::Any
            || comment.len() > 128
            || comment
                .chars()
                .any(|ch| ch.is_control() || matches!(ch, '"' | '\\' | ';' | '{' | '}'))
            || lower.contains("flush ruleset")
            || lower.contains("table inet")
        {
            return Err(SecurityError::InvalidRule);
        }
        Ok(Self {
            id,
            protocol,
            ports,
            source,
            action,
            comment,
            enabled,
        })
    }

    /// Stable rule identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Transport protocol.
    pub fn protocol(&self) -> Protocol {
        self.protocol
    }

    /// Inclusive ports.
    pub fn ports(&self) -> PortRange {
        self.ports
    }

    /// Source network.
    pub fn source(&self) -> NetworkCidr {
        self.source
    }

    /// Allow or deny action.
    pub fn action(&self) -> RuleAction {
        self.action
    }

    /// Safe display comment.
    pub fn comment(&self) -> &str {
        &self.comment
    }

    /// Whether the rule participates in candidates.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enable or disable this rule.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Render one statement for the isolated OpenPanel input chain.
    pub fn render_nft(&self) -> String {
        let protocol = match self.protocol {
            Protocol::Tcp => "tcp",
            Protocol::Udp => "udp",
            Protocol::Any => "",
        };
        let action = match self.action {
            RuleAction::Allow => "accept",
            RuleAction::Deny => "drop",
        };
        let family = match self.source.network {
            IpAddr::V4(_) => "ip",
            IpAddr::V6(_) => "ip6",
        };
        format!(
            "{family} saddr {} {protocol} dport {} {action} comment \"{}\"",
            self.source, self.ports, self.comment
        )
    }
}

/// Lockout safety rules for protected SSH and panel ports.
pub struct FirewallPolicy {
    protected_ports: Vec<u16>,
}
impl FirewallPolicy {
    /// Create a policy with at least one valid protected port.
    pub fn new(mut protected_ports: Vec<u16>) -> Result<Self, SecurityError> {
        if protected_ports.is_empty() || protected_ports.contains(&0) {
            return Err(SecurityError::InvalidPort);
        }
        protected_ports.sort_unstable();
        protected_ports.dedup();
        Ok(Self { protected_ports })
    }

    /// Validate that no broad enabled deny blocks a protected port without break glass.
    pub fn validate_candidate(
        &self,
        rules: &[FirewallRule],
        break_glass: Option<&BreakGlassConfirmation>,
    ) -> Result<(), SecurityError> {
        let risky = rules.iter().any(|rule| {
            rule.enabled
                && rule.action == RuleAction::Deny
                && rule.source.is_all()
                && self
                    .protected_ports
                    .iter()
                    .any(|port| rule.ports.contains(*port))
        });
        if risky && break_glass.is_none_or(|confirmation| !confirmation.is_valid(Utc::now())) {
            Err(SecurityError::LockoutRisk)
        } else {
            Ok(())
        }
    }
}

/// Short-lived authorization for a deliberately risky firewall candidate.
pub struct BreakGlassConfirmation {
    expires_at: DateTime<Utc>,
}
impl BreakGlassConfirmation {
    /// Issue a confirmation with a bounded lifetime.
    pub fn issue(now: DateTime<Utc>, lifetime: Duration) -> Self {
        let seconds = i64::try_from(lifetime.as_secs().min(300)).unwrap_or(300);
        Self {
            expires_at: now + chrono::Duration::seconds(seconds),
        }
    }

    /// Whether confirmation remains valid.
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        now < self.expires_at
    }
}

/// Normalized account or address throttle key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LoginKey(String);
impl LoginKey {
    /// Normalize an account identifier without revealing account existence.
    pub fn account(value: &str) -> Result<Self, SecurityError> {
        let normalized = value.trim().to_lowercase();
        if normalized.is_empty()
            || normalized.len() > 254
            || normalized.chars().any(char::is_control)
        {
            return Err(SecurityError::InvalidLoginKey);
        }
        Ok(Self(format!("account:{normalized}")))
    }

    /// Create an address key.
    pub fn ip(address: IpAddr) -> Self {
        Self(format!("ip:{address}"))
    }

    /// Restore a namespaced durable key.
    pub fn stored(value: &str) -> Result<Self, SecurityError> {
        if !(value.starts_with("account:") || value.starts_with("ip:"))
            || value.len() > 300
            || value.chars().any(char::is_control)
        {
            return Err(SecurityError::InvalidLoginKey);
        }
        Ok(Self(value.to_string()))
    }

    /// Normalized value without its internal key namespace.
    pub fn as_str(&self) -> &str {
        self.0.split_once(':').map_or(&self.0, |(_, value)| value)
    }

    /// Namespaced durable form.
    pub fn storage_key(&self) -> &str {
        &self.0
    }
}

/// One durable temporary login block.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporaryBlock {
    key: LoginKey,
    started_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    unblocked_at: Option<DateTime<Utc>>,
}
impl TemporaryBlock {
    /// Start a nonzero block.
    pub fn new(
        key: LoginKey,
        now: DateTime<Utc>,
        duration: Duration,
    ) -> Result<Self, SecurityError> {
        if duration.is_zero() {
            return Err(SecurityError::InvalidBlock);
        }
        let seconds = i64::try_from(duration.as_secs()).map_err(|_| SecurityError::InvalidBlock)?;
        Ok(Self {
            key,
            started_at: now,
            expires_at: now + chrono::Duration::seconds(seconds),
            unblocked_at: None,
        })
    }

    /// Whether enforcement applies at this instant.
    pub fn is_active(&self, now: DateTime<Utc>) -> bool {
        self.unblocked_at.is_none() && now >= self.started_at && now < self.expires_at
    }

    /// End the block immediately.
    pub fn unblock(&mut self, now: DateTime<Utc>) {
        self.unblocked_at = Some(now);
    }

    /// Block key.
    pub fn key(&self) -> &LoginKey {
        &self.key
    }

    /// Block start.
    pub fn started_at(&self) -> DateTime<Utc> {
        self.started_at
    }

    /// Automatic expiry.
    pub fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }

    /// Manual unblock instant.
    pub fn unblocked_at(&self) -> Option<DateTime<Utc>> {
        self.unblocked_at
    }
}

/// Bounded exponential login-throttle configuration.
pub struct LoginThrottlePolicy {
    attempts: u32,
    window: Duration,
    base: Duration,
    maximum: Duration,
}
impl LoginThrottlePolicy {
    /// Validate thresholds and durations.
    pub fn new(
        attempts: u32,
        window: Duration,
        base: Duration,
        maximum: Duration,
    ) -> Result<Self, SecurityError> {
        if attempts == 0 || window.is_zero() || base.is_zero() || maximum < base {
            return Err(SecurityError::InvalidThrottlePolicy);
        }
        Ok(Self {
            attempts,
            window,
            base,
            maximum,
        })
    }

    /// Exponential duration capped at the configured maximum.
    pub fn block_duration(&self, offense: u32) -> Duration {
        let exponent = offense.saturating_sub(1).min(63);
        let factor = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
        Duration::from_secs(
            self.base
                .as_secs()
                .saturating_mul(factor)
                .min(self.maximum.as_secs()),
        )
    }

    /// Attempts allowed in the window.
    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    /// Observation window.
    pub fn window(&self) -> Duration {
        self.window
    }
}

/// Resolve the client address, honoring forwarding only from trusted proxy networks.
pub fn trusted_client_ip(
    peer: IpAddr,
    forwarded: Option<IpAddr>,
    trusted_proxies: &[NetworkCidr],
) -> IpAddr {
    if trusted_proxies.iter().any(|network| network.contains(peer)) {
        forwarded.unwrap_or(peer)
    } else {
        peer
    }
}

#[cfg(test)]
mod tests {
    use std::{net::IpAddr, time::Duration};

    use chrono::{TimeZone, Utc};
    use proptest::prelude::*;
    use uuid::Uuid;

    use super::*;

    #[test]
    fn cidr_and_port_ranges_validate_ipv4_ipv6_and_ordering() {
        assert_eq!(
            NetworkCidr::parse("192.0.2.0/24").unwrap().to_string(),
            "192.0.2.0/24"
        );
        assert_eq!(
            NetworkCidr::parse("2001:db8::/32").unwrap().to_string(),
            "2001:db8::/32"
        );
        assert!(NetworkCidr::parse("192.0.2.1/33").is_err());
        assert!(NetworkCidr::parse("2001:db8::/129").is_err());
        assert_eq!(PortRange::new(443, 443).unwrap().to_string(), "443");
        assert_eq!(PortRange::new(8000, 8080).unwrap().to_string(), "8000-8080");
        assert!(PortRange::new(8080, 8000).is_err());
        assert!(PortRange::new(0, 22).is_err());
    }

    #[test]
    fn firewall_rule_rejects_ambiguous_or_injectable_values() {
        let source = NetworkCidr::parse("0.0.0.0/0").unwrap();
        let port = PortRange::new(443, 443).unwrap();
        let rule = FirewallRule::new(
            Uuid::new_v4(),
            Protocol::Tcp,
            port,
            source,
            RuleAction::Allow,
            "HTTPS",
            true,
        )
        .unwrap();
        assert_eq!(rule.protocol(), Protocol::Tcp);
        assert!(
            FirewallRule::new(
                Uuid::new_v4(),
                Protocol::Tcp,
                port,
                source,
                RuleAction::Allow,
                "bad\nflush ruleset",
                true
            )
            .is_err()
        );
        assert!(
            FirewallRule::new(
                Uuid::new_v4(),
                Protocol::Any,
                port,
                source,
                RuleAction::Allow,
                "ambiguous",
                true
            )
            .is_err()
        );
    }

    #[test]
    fn protected_panel_and_ssh_ports_require_effective_path_or_break_glass() {
        let policy = FirewallPolicy::new(vec![22, 8443]).unwrap();
        let deny_panel = FirewallRule::new(
            Uuid::new_v4(),
            Protocol::Tcp,
            PortRange::new(8443, 8443).unwrap(),
            NetworkCidr::parse("0.0.0.0/0").unwrap(),
            RuleAction::Deny,
            "deny panel",
            true,
        )
        .unwrap();
        assert_eq!(
            policy.validate_candidate(std::slice::from_ref(&deny_panel), None),
            Err(SecurityError::LockoutRisk)
        );
        let token = BreakGlassConfirmation::issue(Utc::now(), Duration::from_secs(60));
        assert!(
            policy
                .validate_candidate(&[deny_panel], Some(&token))
                .is_ok()
        );
        assert!(policy.validate_candidate(&[], None).is_ok());
    }

    #[test]
    fn login_keys_normalize_and_temporary_blocks_expire_or_unblock() {
        assert_eq!(
            LoginKey::account("  Alice@EXAMPLE.COM ").unwrap().as_str(),
            "alice@example.com"
        );
        let now = Utc.with_ymd_and_hms(2026, 8, 9, 2, 0, 0).unwrap();
        let mut block = TemporaryBlock::new(
            LoginKey::ip("192.0.2.9".parse().unwrap()),
            now,
            Duration::from_secs(30),
        )
        .unwrap();
        assert!(block.is_active(now + chrono::Duration::seconds(29)));
        assert!(!block.is_active(now + chrono::Duration::seconds(30)));
        block.unblock(now + chrono::Duration::seconds(5));
        assert!(!block.is_active(now + chrono::Duration::seconds(6)));
    }

    #[test]
    fn exponential_backoff_is_monotonic_and_bounded() {
        let policy = LoginThrottlePolicy::new(
            5,
            Duration::from_secs(300),
            Duration::from_secs(30),
            Duration::from_secs(3600),
        )
        .unwrap();
        assert_eq!(policy.block_duration(1), Duration::from_secs(30));
        assert_eq!(policy.block_duration(2), Duration::from_secs(60));
        assert_eq!(policy.block_duration(100), Duration::from_secs(3600));
    }

    #[test]
    fn forwarding_header_is_used_only_for_a_trusted_proxy_peer() {
        let trusted = vec![NetworkCidr::parse("10.0.0.0/8").unwrap()];
        let forwarded: IpAddr = "198.51.100.9".parse().unwrap();
        assert_eq!(
            trusted_client_ip("10.1.2.3".parse().unwrap(), Some(forwarded), &trusted),
            forwarded
        );
        let peer: IpAddr = "203.0.113.7".parse().unwrap();
        assert_eq!(trusted_client_ip(peer, Some(forwarded), &trusted), peer);
    }

    proptest! {
        #[test]
        fn prop_account_normalization_is_stable(value in "[ A-Za-z0-9@._+-]{1,80}") {
            if let Ok(first) = LoginKey::account(&value) {
                prop_assert_eq!(LoginKey::account(first.as_str()).unwrap(), first);
            }
        }

        #[test]
        fn prop_backoff_is_bounded_and_monotonic(a in 0_u32..1000, b in 0_u32..1000) {
            let policy = LoginThrottlePolicy::new(5, Duration::from_secs(60), Duration::from_secs(1), Duration::from_secs(3600)).unwrap();
            let low = a.min(b);
            let high = a.max(b);
            prop_assert!(policy.block_duration(low) <= policy.block_duration(high));
            prop_assert!(policy.block_duration(high) <= Duration::from_secs(3600));
        }

        #[test]
        fn prop_comments_cannot_escape_generated_rule(comment in any::<String>()) {
            let result = FirewallRule::new(Uuid::new_v4(), Protocol::Tcp, PortRange::new(443, 443).unwrap(), NetworkCidr::parse("0.0.0.0/0").unwrap(), RuleAction::Allow, comment, true);
            if let Ok(rule) = result {
                let rendered = rule.render_nft();
                prop_assert_eq!(rendered.lines().count(), 1);
                prop_assert!(!rendered.contains("flush ruleset"));
                prop_assert!(!rendered.contains("table inet"));
            }
        }
    }
}
