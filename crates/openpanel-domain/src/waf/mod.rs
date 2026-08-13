//! Per-site web application firewall rules and persistence ports.

use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::RepoError;

/// Errors raised while validating or applying a WAF rule set.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WafError {
    /// The caller is not allowed to manage WAF policy.
    #[error("forbidden")]
    Forbidden,
    /// The requested site does not exist.
    #[error("site not found: {0}")]
    SiteNotFound(String),
    /// A rule or rule set violates a domain invariant.
    #[error("invalid WAF rule: {0}")]
    Invalid(String),
    /// The generated nginx configuration did not validate.
    #[error("WAF configuration validation failed: {0}")]
    Compile(String),
    /// Persistence failed.
    #[error("WAF persistence failed: {0}")]
    Persistence(String),
}

/// Action used when no rule overrides the rule-set default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefaultAction {
    /// Permit the request.
    Allow,
    /// Require a challenge response.
    Challenge,
    /// Reject the request.
    Deny,
}

/// Resulting action for a simulated or matched rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleAction {
    /// Permit the request.
    Allow,
    /// Require a challenge response.
    Challenge,
    /// Reject the request.
    Deny,
}

/// Header challenge behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HeaderChallenge {
    /// Delay the client without redirecting it.
    Tarpit,
    /// Redirect the request to an internal challenge path.
    Redirect {
        /// Absolute request path for the challenge page.
        path: String,
    },
}

/// Closed, strictly deserialized WAF rule schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Rule {
    /// Request-rate limit by client address.
    RateLimit {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// Safe nginx zone name.
        zone: String,
        /// Requests per second.
        rate: u32,
        /// Allowed burst size.
        burst: u32,
        /// Whether nginx should avoid delaying burst requests.
        nodelay: bool,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Concurrent-connection limit by client address.
    ConnLimit {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// Safe nginx zone name.
        zone: String,
        /// Maximum concurrent connections per address.
        per_ip: u32,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Country-code deny/challenge list.
    GeoBlock {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// ISO 3166-1 alpha-2 country codes.
        countries: Vec<String>,
        /// Match action.
        action: RuleAction,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Substring match against the User-Agent header.
    UserAgentBlock {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// Bounded literal pattern.
        pattern: String,
        /// Match action.
        action: RuleAction,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Prefix match against a request path and optional method.
    PathBlock {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// Absolute path prefix.
        pattern: String,
        /// Optional uppercase HTTP method.
        method: Option<String>,
        /// Match action.
        action: RuleAction,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Exact header-value challenge.
    HeaderChallenge {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// HTTP header name.
        header: String,
        /// Exact header value.
        value: String,
        /// Challenge behavior.
        challenge: HeaderChallenge,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
    /// Maximum accepted request-body size.
    BodySizeCap {
        /// Stable rule identifier.
        id: Uuid,
        /// Whether the rule participates in compilation.
        enabled: bool,
        /// Lower numbers compile first.
        priority: u16,
        /// Maximum body size in bytes.
        max_bytes: u64,
        /// Number of observed matches.
        #[serde(default)]
        hit_count: u64,
        /// Timestamp of the latest observed match.
        #[serde(default)]
        last_triggered_at: Option<DateTime<Utc>>,
    },
}

impl Rule {
    /// Stable rule id.
    pub fn id(&self) -> Uuid {
        match self {
            Self::RateLimit { id, .. }
            | Self::ConnLimit { id, .. }
            | Self::GeoBlock { id, .. }
            | Self::UserAgentBlock { id, .. }
            | Self::PathBlock { id, .. }
            | Self::HeaderChallenge { id, .. }
            | Self::BodySizeCap { id, .. } => *id,
        }
    }

    /// Compilation priority.
    pub fn priority(&self) -> u16 {
        match self {
            Self::RateLimit { priority, .. }
            | Self::ConnLimit { priority, .. }
            | Self::GeoBlock { priority, .. }
            | Self::UserAgentBlock { priority, .. }
            | Self::PathBlock { priority, .. }
            | Self::HeaderChallenge { priority, .. }
            | Self::BodySizeCap { priority, .. } => *priority,
        }
    }

    /// Whether the rule participates in compilation and simulation.
    pub fn enabled(&self) -> bool {
        match self {
            Self::RateLimit { enabled, .. }
            | Self::ConnLimit { enabled, .. }
            | Self::GeoBlock { enabled, .. }
            | Self::UserAgentBlock { enabled, .. }
            | Self::PathBlock { enabled, .. }
            | Self::HeaderChallenge { enabled, .. }
            | Self::BodySizeCap { enabled, .. } => *enabled,
        }
    }

    /// Change whether the rule participates in compilation.
    pub fn set_enabled(&mut self, value: bool) {
        match self {
            Self::RateLimit { enabled, .. }
            | Self::ConnLimit { enabled, .. }
            | Self::GeoBlock { enabled, .. }
            | Self::UserAgentBlock { enabled, .. }
            | Self::PathBlock { enabled, .. }
            | Self::HeaderChallenge { enabled, .. }
            | Self::BodySizeCap { enabled, .. } => *enabled = value,
        }
    }

    /// Stable lower-case kind label used by metrics and API responses.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::RateLimit { .. } => "rate_limit",
            Self::ConnLimit { .. } => "conn_limit",
            Self::GeoBlock { .. } => "geo_block",
            Self::UserAgentBlock { .. } => "user_agent_block",
            Self::PathBlock { .. } => "path_block",
            Self::HeaderChallenge { .. } => "header_challenge",
            Self::BodySizeCap { .. } => "body_size_cap",
        }
    }

    /// Current observed match count.
    pub fn hit_count(&self) -> u64 {
        match self {
            Self::RateLimit { hit_count, .. }
            | Self::ConnLimit { hit_count, .. }
            | Self::GeoBlock { hit_count, .. }
            | Self::UserAgentBlock { hit_count, .. }
            | Self::PathBlock { hit_count, .. }
            | Self::HeaderChallenge { hit_count, .. }
            | Self::BodySizeCap { hit_count, .. } => *hit_count,
        }
    }

    /// Latest observed match timestamp.
    pub fn last_triggered_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::RateLimit {
                last_triggered_at, ..
            }
            | Self::ConnLimit {
                last_triggered_at, ..
            }
            | Self::GeoBlock {
                last_triggered_at, ..
            }
            | Self::UserAgentBlock {
                last_triggered_at, ..
            }
            | Self::PathBlock {
                last_triggered_at, ..
            }
            | Self::HeaderChallenge {
                last_triggered_at, ..
            }
            | Self::BodySizeCap {
                last_triggered_at, ..
            } => *last_triggered_at,
        }
    }

    /// Validate all kind-specific bounds and injection boundaries.
    pub fn validate(&self) -> Result<(), WafError> {
        match self {
            Self::RateLimit {
                zone, rate, burst, ..
            } => {
                validate_zone(zone)?;
                if *rate == 0 || *rate > 100_000 || *burst > 1_000_000 {
                    return Err(WafError::Invalid("rate or burst is out of range".into()));
                }
            }
            Self::ConnLimit { zone, per_ip, .. } => {
                validate_zone(zone)?;
                if *per_ip == 0 || *per_ip > 1_000_000 {
                    return Err(WafError::Invalid("connection limit is out of range".into()));
                }
            }
            Self::GeoBlock { countries, .. } => {
                if countries.is_empty()
                    || countries.len() > 249
                    || countries.iter().any(|country| {
                        country.len() != 2 || !country.bytes().all(|byte| byte.is_ascii_uppercase())
                    })
                {
                    return Err(WafError::Invalid(
                        "country codes must be uppercase ISO alpha-2".into(),
                    ));
                }
            }
            Self::UserAgentBlock { pattern, .. } => validate_literal(pattern, 256, false)?,
            Self::PathBlock {
                pattern, method, ..
            } => {
                validate_literal(pattern, 512, true)?;
                if !pattern.starts_with('/') {
                    return Err(WafError::Invalid("path pattern must start with /".into()));
                }
                if let Some(method) = method
                    && (method.is_empty()
                        || method.len() > 16
                        || !method.bytes().all(|byte| byte.is_ascii_uppercase()))
                {
                    return Err(WafError::Invalid("method must be uppercase ASCII".into()));
                }
            }
            Self::HeaderChallenge {
                header,
                value,
                challenge,
                ..
            } => {
                if header.is_empty()
                    || header.len() > 128
                    || !header
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                {
                    return Err(WafError::Invalid("invalid HTTP header name".into()));
                }
                validate_literal(value, 256, false)?;
                if let HeaderChallenge::Redirect { path } = challenge {
                    validate_literal(path, 512, true)?;
                    if !path.starts_with('/') {
                        return Err(WafError::Invalid("redirect path must start with /".into()));
                    }
                }
            }
            Self::BodySizeCap { max_bytes, .. } => {
                if *max_bytes == 0 || *max_bytes > 1024 * 1024 * 1024 {
                    return Err(WafError::Invalid("body-size cap is out of range".into()));
                }
            }
        }
        Ok(())
    }

    fn action(&self) -> RuleAction {
        match self {
            Self::GeoBlock { action, .. }
            | Self::UserAgentBlock { action, .. }
            | Self::PathBlock { action, .. } => *action,
            Self::HeaderChallenge { .. } => RuleAction::Challenge,
            Self::RateLimit { .. } | Self::ConnLimit { .. } | Self::BodySizeCap { .. } => {
                RuleAction::Deny
            }
        }
    }

    fn matches(&self, request: &SimulatedRequest) -> bool {
        if !self.enabled() {
            return false;
        }
        match self {
            Self::RateLimit { .. } | Self::ConnLimit { .. } => false,
            Self::GeoBlock { countries, .. } => request
                .country
                .as_ref()
                .is_some_and(|country| countries.contains(country)),
            Self::UserAgentBlock { pattern, .. } => request.user_agent.contains(pattern),
            Self::PathBlock {
                pattern, method, ..
            } => {
                request.path.starts_with(pattern)
                    && method.as_ref().is_none_or(|value| value == &request.method)
            }
            Self::HeaderChallenge { header, value, .. } => request
                .headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(header))
                .is_some_and(|(_, actual)| actual == value),
            Self::BodySizeCap { max_bytes, .. } => request.body_bytes > *max_bytes,
        }
    }

    fn record_hit(&mut self, at: DateTime<Utc>) {
        match self {
            Self::RateLimit {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::ConnLimit {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::GeoBlock {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::UserAgentBlock {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::PathBlock {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::HeaderChallenge {
                hit_count,
                last_triggered_at,
                ..
            }
            | Self::BodySizeCap {
                hit_count,
                last_triggered_at,
                ..
            } => {
                *hit_count = hit_count.saturating_add(1);
                *last_triggered_at = Some(at);
            }
        }
    }
}

fn validate_zone(zone: &str) -> Result<(), WafError> {
    if zone.is_empty()
        || zone.len() > 48
        || !zone
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(WafError::Invalid("invalid nginx zone name".into()));
    }
    Ok(())
}

fn validate_literal(value: &str, max: usize, allow_space: bool) -> Result<(), WafError> {
    let safe = !value.is_empty()
        && value.len() <= max
        && value.bytes().all(|byte| {
            !matches!(byte, b';' | b'{' | b'}' | b'\n' | b'\r' | 0) && (allow_space || byte != b' ')
        });
    if !safe {
        return Err(WafError::Invalid(
            "value contains unsafe nginx syntax".into(),
        ));
    }
    Ok(())
}

/// Versioned, ordered WAF policy attached to one site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleSet {
    site_id: Uuid,
    version: u64,
    default_action: DefaultAction,
    rules: Vec<Rule>,
}

impl RuleSet {
    /// Validate and construct a deterministic rule set.
    pub fn new(
        site_id: Uuid,
        version: u64,
        default_action: DefaultAction,
        mut rules: Vec<Rule>,
    ) -> Result<Self, WafError> {
        if version == 0 {
            return Err(WafError::Invalid("version must be positive".into()));
        }
        if rules.len() > 256 {
            return Err(WafError::Invalid(
                "a ruleset may contain at most 256 rules".into(),
            ));
        }
        let mut ids = HashSet::with_capacity(rules.len());
        for rule in &rules {
            rule.validate()?;
            if !ids.insert(rule.id()) {
                return Err(WafError::Invalid("duplicate rule id".into()));
            }
        }
        rules.sort_by_key(|rule| (rule.priority(), rule.id()));
        Ok(Self {
            site_id,
            version,
            default_action,
            rules,
        })
    }

    /// Empty initial policy for a site.
    pub fn empty(site_id: Uuid) -> Self {
        Self {
            site_id,
            version: 1,
            default_action: DefaultAction::Allow,
            rules: Vec::new(),
        }
    }

    /// Revalidate a deserialized rule set and restore deterministic ordering.
    pub fn validated(self) -> Result<Self, WafError> {
        Self::new(self.site_id, self.version, self.default_action, self.rules)
    }

    /// Site identifier.
    pub fn site_id(&self) -> Uuid {
        self.site_id
    }

    /// Optimistic policy version.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Default action.
    pub fn default_action(&self) -> DefaultAction {
        self.default_action
    }

    /// Ordered rules.
    pub fn rules(&self) -> &[Rule] {
        &self.rules
    }

    /// Increment one rule's hit metadata.
    pub fn record_hit(&mut self, rule_id: Uuid, at: DateTime<Utc>) -> Result<(), WafError> {
        let rule = self
            .rules
            .iter_mut()
            .find(|rule| rule.id() == rule_id)
            .ok_or_else(|| WafError::Invalid(format!("unknown rule {rule_id}")))?;
        rule.record_hit(at);
        Ok(())
    }
}

/// Canonical request fixture used by the non-mutating dry-run endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SimulatedRequest {
    /// Absolute request path.
    pub path: String,
    /// Uppercase HTTP method.
    pub method: String,
    /// User-Agent value.
    pub user_agent: String,
    /// Optional ISO alpha-2 country code.
    pub country: Option<String>,
    /// Request headers.
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// Request body size.
    #[serde(default)]
    pub body_bytes: u64,
}

/// Dry-run input containing one rule and one canonical request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DryRunRequest {
    /// Rule to compile and simulate.
    pub rule: Rule,
    /// Canonical request fixture.
    pub request: SimulatedRequest,
}

impl DryRunRequest {
    /// Validate the fixture and calculate matching behavior without mutation.
    pub fn simulate(&self) -> Result<DryRunMatch, WafError> {
        self.rule.validate()?;
        validate_literal(&self.request.path, 2048, true)?;
        if !self.request.path.starts_with('/') {
            return Err(WafError::Invalid("request path must start with /".into()));
        }
        Ok(DryRunMatch {
            would_match: self.rule.matches(&self.request),
            action: self.rule.action(),
        })
    }
}

/// Match portion of a dry-run response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunMatch {
    /// Whether the rule matches the canonical request.
    pub would_match: bool,
    /// Action the rule would take when matched.
    pub action: RuleAction,
}

/// Persisted per-rule hit row exposed by REST, CLI, and web surfaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WafHit {
    /// Site identifier.
    pub site_id: Uuid,
    /// Rule identifier.
    pub rule_id: Uuid,
    /// Rule kind label.
    pub kind: String,
    /// Match action.
    pub action: RuleAction,
    /// Cumulative hit count.
    pub count: u64,
    /// Latest trigger timestamp.
    pub last_triggered_at: DateTime<Utc>,
}

/// Persistence port for WAF policy and hit time-series rows.
#[async_trait]
pub trait WafRepository: Send + Sync + 'static {
    /// Load the rule set for a site.
    async fn get(&self, site_id: Uuid) -> Result<Option<RuleSet>, RepoError>;
    /// Atomically replace the rule set for a site.
    async fn put(&self, ruleset: &RuleSet) -> Result<(), RepoError>;
    /// Append or aggregate one observed hit.
    async fn record_hit(&self, hit: &WafHit) -> Result<(), RepoError>;
    /// List current per-rule hit totals for a site.
    async fn hits(&self, site_id: Uuid) -> Result<Vec<WafHit>, RepoError>;
}
