//! Notification channel policy and durable delivery state.

use std::{collections::BTreeSet, fmt};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

use crate::RepoError;

/// Domain validation or state-transition failure.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NotificationError {
    /// A channel field is invalid.
    #[error("invalid notification channel: {0}")]
    InvalidChannel(String),
    /// A subscription field or filter is invalid.
    #[error("invalid notification subscription: {0}")]
    InvalidSubscription(String),
    /// An event is malformed or exceeds its configured bound.
    #[error("invalid notification event: {0}")]
    InvalidEvent(String),
    /// A delivery transition is not valid from its current state.
    #[error("invalid delivery transition")]
    InvalidTransition,
    /// Requested entity was not found.
    #[error("notification entity not found")]
    NotFound,
    /// Actor is not allowed to perform this operation.
    #[error("notification operation forbidden")]
    Forbidden,
    /// Persistence adapter failed.
    #[error("notification persistence failed: {0}")]
    Persistence(String),
}

/// Registered channel variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelKind {
    /// SMTP email transport.
    Smtp,
    /// HMAC-signed HTTPS webhook.
    Webhook,
}

/// SMTP transport security.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TlsMode {
    /// Direct TLS, normally port 465.
    Tls,
    /// SMTP STARTTLS upgrade, normally port 587.
    StartTls,
    /// Plain SMTP, intended only for explicitly trusted local relays.
    None,
}

/// SMTP channel settings.
#[derive(Debug, Clone)]
pub struct SmtpChannel {
    host: String,
    port: u16,
    username: String,
    password_enc: String,
    from_addr: String,
    tls_mode: TlsMode,
    allowlist: BTreeSet<String>,
}

/// Webhook channel settings.
#[derive(Debug, Clone)]
pub struct WebhookChannel {
    url: String,
    secret_enc: String,
    allowlist: BTreeSet<String>,
}

#[derive(Debug, Clone)]
enum ChannelConfig {
    Smtp(SmtpChannel),
    Webhook(WebhookChannel),
}

/// Owner-configured delivery channel.
#[derive(Debug, Clone)]
pub struct Channel {
    id: Uuid,
    name: String,
    config: ChannelConfig,
    created_at: DateTime<Utc>,
    disabled_at: Option<DateTime<Utc>>,
}

/// Secret-free channel view.
#[derive(Debug, Clone, Serialize)]
pub struct ChannelMetadata {
    /// Channel identifier.
    pub id: Uuid,
    /// Operator label.
    pub name: String,
    /// Transport kind.
    pub kind: ChannelKind,
    /// SMTP host or webhook URL.
    pub endpoint: String,
    /// Configured allowlist.
    pub allowlist: Vec<String>,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Disable time.
    pub disabled_at: Option<DateTime<Utc>>,
}

impl Channel {
    /// Restore channel disable state after validated reconstruction.
    pub fn restore_disabled_at(mut self, disabled_at: Option<DateTime<Utc>>) -> Self {
        self.disabled_at = disabled_at;
        self
    }

    /// Construct an SMTP channel whose password is already encrypted.
    #[allow(clippy::too_many_arguments)]
    pub fn smtp(
        id: Uuid,
        name: String,
        host: String,
        port: u16,
        username: String,
        password_enc: String,
        from_addr: String,
        tls_mode: TlsMode,
        allowlist: Vec<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, NotificationError> {
        validate_name(&name)?;
        if !valid_host(&host) || port == 0 || username.is_empty() || password_enc.is_empty() {
            return Err(NotificationError::InvalidChannel(
                "SMTP endpoint or credentials".into(),
            ));
        }
        if !valid_email(&from_addr) {
            return Err(NotificationError::InvalidChannel(
                "SMTP from address".into(),
            ));
        }
        let allowlist = allowlist
            .into_iter()
            .map(|address| {
                let normalized = address.trim().to_ascii_lowercase();
                if valid_email(&normalized) {
                    Ok(normalized)
                } else {
                    Err(NotificationError::InvalidChannel(
                        "SMTP allowlist address".into(),
                    ))
                }
            })
            .collect::<Result<BTreeSet<_>, _>>()?;
        if allowlist.is_empty() {
            return Err(NotificationError::InvalidChannel(
                "empty SMTP allowlist".into(),
            ));
        }
        Ok(Self {
            id,
            name: name.trim().to_owned(),
            config: ChannelConfig::Smtp(SmtpChannel {
                host,
                port,
                username,
                password_enc,
                from_addr: from_addr.to_ascii_lowercase(),
                tls_mode,
                allowlist,
            }),
            created_at,
            disabled_at: None,
        })
    }

    /// Construct an HTTPS webhook channel whose signing key is encrypted.
    pub fn webhook(
        id: Uuid,
        name: String,
        url: String,
        secret_enc: String,
        allowlist: Vec<String>,
        created_at: DateTime<Utc>,
    ) -> Result<Self, NotificationError> {
        validate_name(&name)?;
        let parsed = Url::parse(&url)
            .map_err(|_| NotificationError::InvalidChannel("webhook URL".into()))?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || parsed.username() != ""
            || parsed.password().is_some()
            || secret_enc.is_empty()
        {
            return Err(NotificationError::InvalidChannel(
                "secure webhook endpoint".into(),
            ));
        }
        let allowlist = allowlist
            .into_iter()
            .map(|cidr| validate_cidr(&cidr).map(|()| cidr))
            .collect::<Result<BTreeSet<_>, _>>()?;
        if allowlist.is_empty() {
            return Err(NotificationError::InvalidChannel(
                "empty webhook allowlist".into(),
            ));
        }
        Ok(Self {
            id,
            name: name.trim().to_owned(),
            config: ChannelConfig::Webhook(WebhookChannel {
                url,
                secret_enc,
                allowlist,
            }),
            created_at,
            disabled_at: None,
        })
    }

    /// Disable without deleting audit history.
    pub fn disable(&mut self, now: DateTime<Utc>) {
        self.disabled_at = Some(now);
    }

    /// Secret-free metadata.
    pub fn metadata(&self) -> ChannelMetadata {
        let (kind, endpoint, allowlist) = match &self.config {
            ChannelConfig::Smtp(config) => (
                ChannelKind::Smtp,
                format!("{}:{}", config.host, config.port),
                config.allowlist.iter().cloned().collect(),
            ),
            ChannelConfig::Webhook(config) => (
                ChannelKind::Webhook,
                config.url.clone(),
                config.allowlist.iter().cloned().collect(),
            ),
        };
        ChannelMetadata {
            id: self.id,
            name: self.name.clone(),
            kind,
            endpoint,
            allowlist,
            created_at: self.created_at,
            disabled_at: self.disabled_at,
        }
    }

    /// Channel identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Label.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Transport kind.
    pub fn kind(&self) -> ChannelKind {
        match &self.config {
            ChannelConfig::Smtp(_) => ChannelKind::Smtp,
            ChannelConfig::Webhook(_) => ChannelKind::Webhook,
        }
    }

    /// SMTP configuration when applicable.
    pub fn smtp_config(&self) -> Option<&SmtpChannel> {
        match &self.config {
            ChannelConfig::Smtp(value) => Some(value),
            ChannelConfig::Webhook(_) => None,
        }
    }

    /// Webhook configuration when applicable.
    pub fn webhook_config(&self) -> Option<&WebhookChannel> {
        match &self.config {
            ChannelConfig::Webhook(value) => Some(value),
            ChannelConfig::Smtp(_) => None,
        }
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Disable timestamp.
    pub fn disabled_at(&self) -> Option<DateTime<Utc>> {
        self.disabled_at
    }

    /// Whether dispatch is enabled.
    pub fn is_enabled(&self) -> bool {
        self.disabled_at.is_none()
    }
}

impl SmtpChannel {
    /// Relay host.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// Relay port.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Authentication username.
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Encrypted password envelope.
    pub fn password_enc(&self) -> &str {
        &self.password_enc
    }

    /// Sender address.
    pub fn from_addr(&self) -> &str {
        &self.from_addr
    }

    /// Transport security.
    pub fn tls_mode(&self) -> TlsMode {
        self.tls_mode
    }

    /// Whether the exact recipient is allowed.
    pub fn allows_recipient(&self, recipient: &str) -> bool {
        self.allowlist
            .contains(&recipient.trim().to_ascii_lowercase())
    }

    /// Allowlist values.
    pub fn allowlist(&self) -> &BTreeSet<String> {
        &self.allowlist
    }
}

impl WebhookChannel {
    /// HTTPS destination.
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Encrypted signing-key envelope.
    pub fn secret_enc(&self) -> &str {
        &self.secret_enc
    }

    /// Allowed destination networks.
    pub fn allowlist(&self) -> &BTreeSet<String> {
        &self.allowlist
    }

    /// Whether a resolved destination address is inside an allowed network.
    pub fn allows_ip(&self, address: std::net::IpAddr) -> bool {
        self.allowlist
            .iter()
            .any(|cidr| cidr_contains(cidr, address))
    }
}

fn cidr_contains(cidr: &str, address: std::net::IpAddr) -> bool {
    let Some((network, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(network), Ok(prefix)) = (network.parse::<std::net::IpAddr>(), prefix.parse::<u8>())
    else {
        return false;
    };
    match (network, address) {
        (std::net::IpAddr::V4(network), std::net::IpAddr::V4(address)) if prefix <= 32 => {
            let mask = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix)
            };
            u32::from(network) & mask == u32::from(address) & mask
        }
        (std::net::IpAddr::V6(network), std::net::IpAddr::V6(address)) if prefix <= 128 => {
            let mask = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix)
            };
            u128::from(network) & mask == u128::from(address) & mask
        }
        _ => false,
    }
}

fn validate_name(name: &str) -> Result<(), NotificationError> {
    if name.trim().is_empty() || name.len() > 120 || name.chars().any(char::is_control) {
        Err(NotificationError::InvalidChannel("name".into()))
    } else {
        Ok(())
    }
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':'))
}

fn valid_email(value: &str) -> bool {
    value.len() <= 254
        && !value.chars().any(char::is_control)
        && value.split_once('@').is_some_and(|(local, domain)| {
            !local.is_empty() && valid_host(domain) && !domain.contains(':')
        })
}

fn validate_cidr(value: &str) -> Result<(), NotificationError> {
    let (address, prefix) = value
        .split_once('/')
        .ok_or_else(|| NotificationError::InvalidChannel("webhook CIDR".into()))?;
    let address: std::net::IpAddr = address
        .parse()
        .map_err(|_| NotificationError::InvalidChannel("webhook CIDR".into()))?;
    let prefix: u8 = prefix
        .parse()
        .map_err(|_| NotificationError::InvalidChannel("webhook CIDR".into()))?;
    if prefix > if address.is_ipv4() { 32 } else { 128 } {
        return Err(NotificationError::InvalidChannel("webhook CIDR".into()));
    }
    Ok(())
}

/// Subscribable event family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EventKind {
    /// Monitoring or WAF alert.
    Alert,
    /// Security/operator audit event.
    Audit,
    /// Terminal background job.
    JobTerminal,
}

impl EventKind {
    /// Stable wire name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Alert => "alert.fired",
            Self::Audit => "audit.emitted",
            Self::JobTerminal => "job.terminal",
        }
    }
}

/// Event importance used for filter ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Informational event.
    Info,
    /// Operator attention recommended.
    Warning,
    /// Immediate operator action recommended.
    Critical,
}

/// Sanitized event presented to subscriptions and adapters.
#[derive(Debug, Clone)]
pub struct NotificationEvent {
    id: Uuid,
    kind: EventKind,
    subject: String,
    severity: Severity,
    details: serde_json::Value,
    occurred_at: DateTime<Utc>,
}

impl NotificationEvent {
    /// Construct a bounded, secret-redacted event.
    pub fn new(
        id: Uuid,
        kind: EventKind,
        subject: String,
        severity: Severity,
        details: serde_json::Value,
        occurred_at: DateTime<Utc>,
    ) -> Result<Self, NotificationError> {
        if subject.trim().is_empty() || subject.len() > 240 || subject.chars().any(char::is_control)
        {
            return Err(NotificationError::InvalidEvent("subject".into()));
        }
        let details = redact_json(details);
        if serde_json::to_vec(&details).map_or(true, |bytes| bytes.len() > 16 * 1024) {
            return Err(NotificationError::InvalidEvent("details too large".into()));
        }
        Ok(Self {
            id,
            kind,
            subject,
            severity,
            details,
            occurred_at,
        })
    }

    /// Render a deterministic bounded webhook body.
    pub fn webhook_body(
        &self,
        delivery_id: Uuid,
        max_bytes: usize,
    ) -> Result<Vec<u8>, NotificationError> {
        let body = serde_json::to_vec(&serde_json::json!({
            "delivery_id": delivery_id,
            "event": self.kind.as_str(),
            "event_id": self.id,
            "ts": self.occurred_at,
            "subject": self.subject,
            "severity": self.severity,
            "details": self.details,
        }))
        .map_err(|error| NotificationError::InvalidEvent(error.to_string()))?;
        if body.len() > max_bytes {
            return Err(NotificationError::InvalidEvent("payload too large".into()));
        }
        Ok(body)
    }

    /// Event id.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Family.
    pub fn kind(&self) -> EventKind {
        self.kind
    }

    /// Subject.
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// Severity.
    pub fn severity(&self) -> Severity {
        self.severity
    }

    /// Redacted structured details.
    pub fn details(&self) -> &serde_json::Value {
        &self.details
    }

    /// Occurrence time.
    pub fn occurred_at(&self) -> DateTime<Utc> {
        self.occurred_at
    }
}

fn redact_json(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(key, value)| {
                    let lower = key.to_ascii_lowercase();
                    if ["password", "secret", "token", "credential", "private_key"]
                        .iter()
                        .any(|needle| lower.contains(needle))
                    {
                        (key, serde_json::Value::String("[REDACTED]".into()))
                    } else {
                        (key, redact_json(value))
                    }
                })
                .collect(),
        ),
        serde_json::Value::Array(values) => {
            serde_json::Value::Array(values.into_iter().map(redact_json).collect())
        }
        other => other,
    }
}

/// Parsed subscription filter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventFilter {
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    metrics: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    actions: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    kinds: BTreeSet<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    severity_at_least: Option<Severity>,
}

impl EventFilter {
    /// Parse the small schema supported by one event family.
    pub fn from_json(kind: EventKind, value: serde_json::Value) -> Result<Self, NotificationError> {
        let object = value.as_object().ok_or_else(|| {
            NotificationError::InvalidSubscription("filter must be an object".into())
        })?;
        let allowed: &[&str] = match kind {
            EventKind::Alert => &["metrics", "threshold_direction", "severity_at_least"],
            EventKind::Audit => &["actions"],
            EventKind::JobTerminal => &["kinds", "outcome"],
        };
        if object.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(NotificationError::InvalidSubscription(
                "unknown filter field".into(),
            ));
        }
        let metrics = string_set(object.get("metrics"))?;
        let actions = string_set(object.get("actions"))?;
        let kinds = string_set(object.get("kinds"))?;
        let outcome = object
            .get("outcome")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned);
        if outcome
            .as_deref()
            .is_some_and(|value| !matches!(value, "any" | "success" | "failure"))
        {
            return Err(NotificationError::InvalidSubscription("outcome".into()));
        }
        let severity_at_least = object
            .get("severity_at_least")
            .and_then(serde_json::Value::as_str)
            .map(parse_severity)
            .transpose()?;
        Ok(Self {
            metrics,
            actions,
            kinds,
            outcome,
            severity_at_least,
        })
    }

    /// Whether this filter accepts the event.
    pub fn matches(&self, event: &NotificationEvent) -> bool {
        match event.kind {
            EventKind::Alert => {
                !self.metrics.is_empty()
                    && event
                        .details
                        .get("metric")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|metric| self.metrics.contains(metric))
                    && self
                        .severity_at_least
                        .is_none_or(|minimum| event.severity >= minimum)
            }
            EventKind::Audit => {
                !self.actions.is_empty()
                    && event
                        .details
                        .get("action")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|action| self.actions.contains(action))
            }
            EventKind::JobTerminal => {
                !self.kinds.is_empty()
                    && event
                        .details
                        .get("kind")
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|kind| self.kinds.contains(kind))
                    && self.outcome.as_deref().is_none_or(|expected| {
                        expected == "any"
                            || event
                                .details
                                .get("outcome")
                                .and_then(serde_json::Value::as_str)
                                == Some(expected)
                    })
            }
        }
    }

    /// Stable JSON representation for persistence and API metadata.
    pub fn to_json(&self) -> Result<serde_json::Value, NotificationError> {
        serde_json::to_value(self)
            .map_err(|error| NotificationError::InvalidSubscription(error.to_string()))
    }
}

fn string_set(value: Option<&serde_json::Value>) -> Result<BTreeSet<String>, NotificationError> {
    value
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| NotificationError::InvalidSubscription("filter list".into()))?
                .iter()
                .map(|item| {
                    item.as_str()
                        .filter(|text| !text.is_empty() && text.len() <= 80)
                        .map(str::to_owned)
                        .ok_or_else(|| NotificationError::InvalidSubscription("filter item".into()))
                })
                .collect()
        })
        .unwrap_or_else(|| Ok(BTreeSet::new()))
}

fn parse_severity(value: &str) -> Result<Severity, NotificationError> {
    match value {
        "info" => Ok(Severity::Info),
        "warning" => Ok(Severity::Warning),
        "critical" => Ok(Severity::Critical),
        _ => Err(NotificationError::InvalidSubscription("severity".into())),
    }
}

/// Per-user event subscription.
#[derive(Debug, Clone)]
pub struct Subscription {
    id: Uuid,
    user_id: Uuid,
    channel_id: Uuid,
    destination: String,
    kind: EventKind,
    filter: EventFilter,
    enabled: bool,
    created_at: DateTime<Utc>,
}

impl Subscription {
    /// Create an enabled subscription.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        user_id: Uuid,
        channel_id: Uuid,
        destination: String,
        kind: EventKind,
        filter: EventFilter,
        created_at: DateTime<Utc>,
    ) -> Result<Self, NotificationError> {
        if destination.trim().is_empty()
            || destination.len() > 2048
            || destination.chars().any(char::is_control)
        {
            return Err(NotificationError::InvalidSubscription("destination".into()));
        }
        Ok(Self {
            id,
            user_id,
            channel_id,
            destination,
            kind,
            filter,
            enabled: true,
            created_at,
        })
    }

    /// Disable while preserving history.
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Restore persisted enabled state after validated reconstruction.
    pub fn restore_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Match only enabled subscriptions of the same family.
    pub fn matches(&self, event: &NotificationEvent) -> bool {
        self.enabled && self.kind == event.kind && self.filter.matches(event)
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Owner.
    pub fn user_id(&self) -> Uuid {
        self.user_id
    }

    /// Channel.
    pub fn channel_id(&self) -> Uuid {
        self.channel_id
    }

    /// Recipient or webhook endpoint.
    pub fn destination(&self) -> &str {
        &self.destination
    }

    /// Event family.
    pub fn kind(&self) -> EventKind {
        self.kind
    }

    /// Filter.
    pub fn filter(&self) -> &EventFilter {
        &self.filter
    }

    /// Enabled state.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Creation timestamp.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
}

/// Durable delivery lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryStatus {
    /// Ready for lease.
    Pending,
    /// Owned by one dispatcher until lease expiry.
    Leased,
    /// Waiting for retry time.
    Retry,
    /// Adapter accepted the delivery.
    TerminalSuccess,
    /// Retry budget exhausted or permanent rejection.
    TerminalFailure,
}

/// One stable delivery record.
#[derive(Debug, Clone)]
pub struct DeliveryAttempt {
    id: Uuid,
    event_id: Uuid,
    channel_id: Uuid,
    subscription_id: Uuid,
    payload_digest: String,
    status: DeliveryStatus,
    attempt_n: u32,
    next_retry_at: Option<DateTime<Utc>>,
    lease_until: Option<DateTime<Utc>>,
    last_error_redacted: Option<String>,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl DeliveryAttempt {
    /// Create a pending record.
    pub fn pending(
        id: Uuid,
        event_id: Uuid,
        channel_id: Uuid,
        subscription_id: Uuid,
        payload_digest: String,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            event_id,
            channel_id,
            subscription_id,
            payload_digest,
            status: DeliveryStatus::Pending,
            attempt_n: 0,
            next_retry_at: None,
            lease_until: None,
            last_error_redacted: None,
            created_at,
            completed_at: None,
        }
    }

    /// Restore a validated persistence snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn restore(
        id: Uuid,
        event_id: Uuid,
        channel_id: Uuid,
        subscription_id: Uuid,
        payload_digest: String,
        status: DeliveryStatus,
        attempt_n: u32,
        next_retry_at: Option<DateTime<Utc>>,
        lease_until: Option<DateTime<Utc>>,
        last_error_redacted: Option<String>,
        created_at: DateTime<Utc>,
        completed_at: Option<DateTime<Utc>>,
    ) -> Result<Self, NotificationError> {
        if payload_digest.is_empty() || payload_digest.len() > 128 {
            return Err(NotificationError::InvalidEvent("payload digest".into()));
        }
        Ok(Self {
            id,
            event_id,
            channel_id,
            subscription_id,
            payload_digest,
            status,
            attempt_n,
            next_retry_at,
            lease_until,
            last_error_redacted,
            created_at,
            completed_at,
        })
    }

    /// Lease a due record.
    pub fn lease(
        &mut self,
        now: DateTime<Utc>,
        duration: Duration,
    ) -> Result<(), NotificationError> {
        let due = self.status == DeliveryStatus::Pending
            || (self.status == DeliveryStatus::Retry
                && self.next_retry_at.is_some_and(|retry| retry <= now));
        if !due || duration <= Duration::zero() {
            return Err(NotificationError::InvalidTransition);
        }
        self.status = DeliveryStatus::Leased;
        self.lease_until = Some(now + duration);
        Ok(())
    }

    /// Recover an expired lease to pending without changing identity.
    pub fn recover_expired_lease(&mut self, now: DateTime<Utc>) -> bool {
        if self.status == DeliveryStatus::Leased
            && self.lease_until.is_some_and(|until| until < now)
        {
            self.status = DeliveryStatus::Pending;
            self.lease_until = None;
            true
        } else {
            false
        }
    }

    /// Mark adapter acceptance.
    pub fn record_success(&mut self, now: DateTime<Utc>) -> Result<(), NotificationError> {
        if self.status != DeliveryStatus::Leased {
            return Err(NotificationError::InvalidTransition);
        }
        self.status = DeliveryStatus::TerminalSuccess;
        self.completed_at = Some(now);
        self.lease_until = None;
        Ok(())
    }

    /// Schedule the next transient retry or exhaust the five-attempt budget.
    pub fn record_transient_failure(
        &mut self,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<(), NotificationError> {
        if self.status != DeliveryStatus::Leased {
            return Err(NotificationError::InvalidTransition);
        }
        self.attempt_n = self.attempt_n.saturating_add(1);
        self.last_error_redacted = Some(redact_error(error));
        self.lease_until = None;
        if self.attempt_n >= 6 {
            self.status = DeliveryStatus::TerminalFailure;
            self.completed_at = Some(now);
            self.next_retry_at = None;
        } else {
            self.status = DeliveryStatus::Retry;
            self.next_retry_at = Some(now + retry_delay(self.attempt_n));
        }
        Ok(())
    }

    /// Record an immediate permanent failure.
    pub fn record_permanent_failure(
        &mut self,
        now: DateTime<Utc>,
        error: &str,
    ) -> Result<(), NotificationError> {
        if self.status != DeliveryStatus::Leased {
            return Err(NotificationError::InvalidTransition);
        }
        self.attempt_n = self.attempt_n.saturating_add(1);
        self.status = DeliveryStatus::TerminalFailure;
        self.last_error_redacted = Some(redact_error(error));
        self.completed_at = Some(now);
        self.lease_until = None;
        self.next_retry_at = None;
        Ok(())
    }

    /// Identifier.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Event id.
    pub fn event_id(&self) -> Uuid {
        self.event_id
    }

    /// Channel id.
    pub fn channel_id(&self) -> Uuid {
        self.channel_id
    }

    /// Subscription id.
    pub fn subscription_id(&self) -> Uuid {
        self.subscription_id
    }

    /// Stable payload hash.
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }

    /// State.
    pub fn status(&self) -> DeliveryStatus {
        self.status
    }

    /// Number of failed adapter calls.
    pub fn attempt_n(&self) -> u32 {
        self.attempt_n
    }

    /// Next due time.
    pub fn next_retry_at(&self) -> Option<DateTime<Utc>> {
        self.next_retry_at
    }

    /// Lease expiry.
    pub fn lease_until(&self) -> Option<DateTime<Utc>> {
        self.lease_until
    }

    /// Redacted last failure.
    pub fn last_error_redacted(&self) -> Option<&str> {
        self.last_error_redacted.as_deref()
    }

    /// Creation time.
    pub fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    /// Completion time.
    pub fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
}

fn redact_error(error: &str) -> String {
    let bounded: String = error
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect();
    if bounded.to_ascii_lowercase().contains("secret")
        || bounded.to_ascii_lowercase().contains("password")
        || bounded.to_ascii_lowercase().contains("token")
    {
        "[REDACTED DELIVERY ERROR]".into()
    } else {
        bounded
    }
}

/// Fixed five-attempt retry schedule.
pub fn retry_delay(attempt: u32) -> Duration {
    match attempt {
        1 => Duration::minutes(1),
        2 => Duration::minutes(5),
        3 => Duration::minutes(30),
        4 => Duration::hours(2),
        _ => Duration::hours(12),
    }
}

/// Compute the outbound signature over exact raw bytes.
pub fn sign_webhook(secret: &[u8], body: &[u8]) -> String {
    #[allow(clippy::expect_used)]
    let mut mac = Hmac::<Sha256>::new_from_slice(secret)
        .expect("HMAC-SHA256 accepts signing keys of every length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// SHA-256 digest used to bind a stable delivery record to bytes.
pub fn payload_digest(body: &[u8]) -> String {
    hex::encode(Sha256::digest(body))
}

/// Persistence port for channels, subscriptions, events, and deliveries.
#[async_trait]
pub trait NotificationRepository: Send + Sync {
    /// Insert a channel.
    async fn create_channel(&self, channel: &Channel) -> Result<(), RepoError>;
    /// Update channel state or credentials.
    async fn update_channel(&self, channel: &Channel) -> Result<(), RepoError>;
    /// Find a channel.
    async fn find_channel(&self, id: Uuid) -> Result<Option<Channel>, RepoError>;
    /// List channels.
    async fn list_channels(&self) -> Result<Vec<Channel>, RepoError>;
    /// Insert a subscription.
    async fn create_subscription(&self, subscription: &Subscription) -> Result<(), RepoError>;
    /// Update subscription state.
    async fn update_subscription(&self, subscription: &Subscription) -> Result<(), RepoError>;
    /// Find a subscription.
    async fn find_subscription(&self, id: Uuid) -> Result<Option<Subscription>, RepoError>;
    /// List subscriptions owned by one user.
    async fn list_subscriptions(&self, user_id: Uuid) -> Result<Vec<Subscription>, RepoError>;
    /// List enabled subscriptions for an event family.
    async fn matching_subscriptions(&self, kind: EventKind)
    -> Result<Vec<Subscription>, RepoError>;
    /// Persist a sanitized event.
    async fn create_event(&self, event: &NotificationEvent) -> Result<(), RepoError>;
    /// Load an event.
    async fn find_event(&self, id: Uuid) -> Result<Option<NotificationEvent>, RepoError>;
    /// Insert one stable delivery, idempotent by event and subscription.
    async fn create_delivery(&self, delivery: &DeliveryAttempt) -> Result<bool, RepoError>;
    /// Atomically recover expired leases and lease due rows.
    async fn lease_due(
        &self,
        now: DateTime<Utc>,
        limit: u32,
        lease_for: Duration,
    ) -> Result<Vec<DeliveryAttempt>, RepoError>;
    /// Persist a delivery transition.
    async fn update_delivery(&self, delivery: &DeliveryAttempt) -> Result<(), RepoError>;
    /// Find a delivery.
    async fn find_delivery(&self, id: Uuid) -> Result<Option<DeliveryAttempt>, RepoError>;
    /// Increment a subscription's terminal failure counter.
    async fn increment_failure_count(&self, subscription_id: Uuid) -> Result<(), RepoError>;
    /// Aggregate one channel's recent delivery state.
    async fn channel_health(
        &self,
        channel_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<ChannelHealth, RepoError>;
}

/// Rolling delivery health for one channel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelHealth {
    /// Channel identifier.
    pub channel_id: Uuid,
    /// Successful deliveries completed in the window.
    pub delivered: u64,
    /// Failed deliveries completed in the window.
    pub failed: u64,
    /// Currently pending or retrying deliveries.
    pub pending: u64,
    /// Oldest currently pending creation time.
    pub oldest_pending_at: Option<DateTime<Utc>>,
    /// Earliest scheduled retry.
    pub next_retry_at: Option<DateTime<Utc>>,
    /// Whether enabled subscriptions exist but no recent success exists.
    pub degraded: bool,
}

impl fmt::Display for ChannelKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Smtp => "smtp",
            Self::Webhook => "webhook",
        })
    }
}
