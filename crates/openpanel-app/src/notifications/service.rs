//! Notification lifecycle, fan-out, policy enforcement, and dispatch.

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use chrono::{Duration, Utc};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService};
use openpanel_domain::{
    Role, User,
    notifications::{
        Channel, ChannelHealth, ChannelMetadata, DeliveryAttempt, EventFilter, EventKind,
        NotificationError, NotificationEvent, NotificationRepository, Subscription, TlsMode,
        payload_digest,
    },
};
use serde::Serialize;
use uuid::Uuid;

use super::adapter::{AdapterOutcome, NotificationAdapter};
use crate::databases::crypto::{decrypt_from_storage, encrypt_to_storage};

/// SMTP registration input containing plaintext only for this call.
pub struct CreateSmtpChannel {
    /// Operator label.
    pub name: String,
    /// Relay host.
    pub host: String,
    /// Relay port.
    pub port: u16,
    /// Authentication username.
    pub username: String,
    /// Plaintext password.
    pub password: String,
    /// Sender mailbox.
    pub from_addr: String,
    /// TLS policy.
    pub tls_mode: TlsMode,
    /// Exact allowed recipients.
    pub allowlist: Vec<String>,
}

/// Webhook registration input containing plaintext only for this call.
pub struct CreateWebhookChannel {
    /// Operator label.
    pub name: String,
    /// HTTPS URL.
    pub url: String,
    /// Plaintext HMAC signing secret.
    pub signing_secret: String,
    /// Allowed resolved IP networks.
    pub allowlist: Vec<String>,
}

/// Subscription creation input.
pub struct CreateSubscription {
    /// Channel binding.
    pub channel_id: Uuid,
    /// Recipient or exact webhook URL.
    pub destination: String,
    /// Event family.
    pub kind: EventKind,
    /// Family-specific filter JSON.
    pub filter: serde_json::Value,
}

/// Safe subscription response.
#[derive(Debug, Clone, Serialize)]
pub struct SubscriptionView {
    /// Stable identifier.
    pub id: Uuid,
    /// Owner identifier.
    pub user_id: Uuid,
    /// Bound channel.
    pub channel_id: Uuid,
    /// Destination.
    pub destination: String,
    /// Event family.
    pub kind: EventKind,
    /// Filter JSON.
    pub filter: serde_json::Value,
    /// Enabled state.
    pub enabled: bool,
}

/// Application error with safe public messages.
#[derive(Debug, thiserror::Error)]
pub enum NotificationServiceError {
    /// Caller lacks permission.
    #[error("forbidden")]
    Forbidden,
    /// Entity does not exist.
    #[error("notification entity not found")]
    NotFound,
    /// Domain policy rejected input.
    #[error(transparent)]
    Invalid(#[from] NotificationError),
    /// Destination failed allowlist policy.
    #[error("notification destination rejected")]
    DestinationRejected,
    /// Adapter or persistence failure.
    #[error("notification service failed: {0}")]
    Internal(String),
}

/// Notification use-case service.
pub struct NotificationService {
    repo: Arc<dyn NotificationRepository>,
    adapter: Arc<dyn NotificationAdapter>,
    audit: Arc<dyn AuditService>,
    master_key: [u8; 32],
    max_payload_bytes: usize,
    channel_calls: Mutex<HashMap<Uuid, VecDeque<chrono::DateTime<Utc>>>>,
    calls_per_minute: usize,
}

impl NotificationService {
    /// Construct from explicit ports.
    pub fn new(
        repo: Arc<dyn NotificationRepository>,
        adapter: Arc<dyn NotificationAdapter>,
        audit: Arc<dyn AuditService>,
        master_key: [u8; 32],
    ) -> Self {
        Self {
            repo,
            adapter,
            audit,
            master_key,
            max_payload_bytes: 16 * 1024,
            channel_calls: Mutex::new(HashMap::new()),
            calls_per_minute: 60,
        }
    }

    /// Owner-only SMTP registration.
    pub async fn create_smtp(
        &self,
        actor: &User,
        input: CreateSmtpChannel,
    ) -> Result<ChannelMetadata, NotificationServiceError> {
        owner(actor)?;
        let encrypted = encrypt_to_storage(&self.master_key, &input.password).map_err(internal)?;
        let channel = Channel::smtp(
            Uuid::new_v4(),
            input.name,
            input.host,
            input.port,
            input.username,
            encrypted,
            input.from_addr,
            input.tls_mode,
            input.allowlist,
            Utc::now(),
        )?;
        self.repo.create_channel(&channel).await.map_err(internal)?;
        self.audit_change(actor, channel.id()).await;
        Ok(channel.metadata())
    }

    /// Owner-only webhook registration.
    pub async fn create_webhook(
        &self,
        actor: &User,
        input: CreateWebhookChannel,
    ) -> Result<ChannelMetadata, NotificationServiceError> {
        owner(actor)?;
        let encrypted =
            encrypt_to_storage(&self.master_key, &input.signing_secret).map_err(internal)?;
        let channel = Channel::webhook(
            Uuid::new_v4(),
            input.name,
            input.url,
            encrypted,
            input.allowlist,
            Utc::now(),
        )?;
        self.ensure_destination(
            &channel,
            channel.webhook_config().map_or("", |value| value.url()),
        )
        .await?;
        self.repo.create_channel(&channel).await.map_err(internal)?;
        self.audit_change(actor, channel.id()).await;
        Ok(channel.metadata())
    }

    /// Owner-only channel list.
    pub async fn channels(
        &self,
        actor: &User,
    ) -> Result<Vec<ChannelMetadata>, NotificationServiceError> {
        owner(actor)?;
        Ok(self
            .repo
            .list_channels()
            .await
            .map_err(internal)?
            .into_iter()
            .map(|channel| channel.metadata())
            .collect())
    }

    /// Disable a channel while retaining history.
    pub async fn disable_channel(
        &self,
        actor: &User,
        id: Uuid,
    ) -> Result<(), NotificationServiceError> {
        owner(actor)?;
        let mut channel = self
            .repo
            .find_channel(id)
            .await
            .map_err(internal)?
            .ok_or(NotificationServiceError::NotFound)?;
        channel.disable(Utc::now());
        self.repo.update_channel(&channel).await.map_err(internal)?;
        self.audit_change(actor, id).await;
        Ok(())
    }

    /// Owner-only immediate transport test without creating a subscription.
    pub async fn test_channel(
        &self,
        actor: &User,
        id: Uuid,
        destination: &str,
    ) -> Result<(), NotificationServiceError> {
        owner(actor)?;
        let channel = self
            .repo
            .find_channel(id)
            .await
            .map_err(internal)?
            .filter(Channel::is_enabled)
            .ok_or(NotificationServiceError::NotFound)?;
        self.ensure_destination(&channel, destination).await?;
        let event = NotificationEvent::new(
            Uuid::new_v4(),
            EventKind::Audit,
            "OpenPanel notification test".into(),
            openpanel_domain::notifications::Severity::Info,
            serde_json::json!({"action":"notification.test"}),
            Utc::now(),
        )?;
        let delivery_id = Uuid::new_v4();
        let outcome = if let Some(config) = channel.smtp_config() {
            let password =
                decrypt_from_storage(&self.master_key, config.password_enc()).map_err(internal)?;
            self.adapter
                .smtp(config, &password, destination, &event)
                .await
        } else if let Some(config) = channel.webhook_config() {
            let secret =
                decrypt_from_storage(&self.master_key, config.secret_enc()).map_err(internal)?;
            let body = event.webhook_body(delivery_id, self.max_payload_bytes)?;
            self.adapter
                .webhook(config, &secret, delivery_id, &event, &body)
                .await
        } else {
            AdapterOutcome::Permanent("channel kind missing".into())
        };
        match outcome {
            AdapterOutcome::Accepted => Ok(()),
            AdapterOutcome::Transient(error) | AdapterOutcome::Permanent(error) => {
                Err(internal(error))
            }
        }
    }

    /// Create a subscription owned by the caller.
    pub async fn create_subscription(
        &self,
        actor: &User,
        input: CreateSubscription,
    ) -> Result<SubscriptionView, NotificationServiceError> {
        let channel = self
            .repo
            .find_channel(input.channel_id)
            .await
            .map_err(internal)?
            .filter(Channel::is_enabled)
            .ok_or(NotificationServiceError::NotFound)?;
        self.ensure_destination(&channel, &input.destination)
            .await?;
        let filter = EventFilter::from_json(input.kind, input.filter)?;
        let subscription = Subscription::new(
            Uuid::new_v4(),
            actor.id(),
            input.channel_id,
            input.destination,
            input.kind,
            filter,
            Utc::now(),
        )?;
        self.repo
            .create_subscription(&subscription)
            .await
            .map_err(internal)?;
        self.audit_change(actor, subscription.id()).await;
        view(&subscription)
    }

    /// List only the caller's subscriptions.
    pub async fn subscriptions(
        &self,
        actor: &User,
    ) -> Result<Vec<SubscriptionView>, NotificationServiceError> {
        self.repo
            .list_subscriptions(actor.id())
            .await
            .map_err(internal)?
            .iter()
            .map(view)
            .collect()
    }

    /// Disable only a caller-owned subscription.
    pub async fn disable_subscription(
        &self,
        actor: &User,
        id: Uuid,
    ) -> Result<(), NotificationServiceError> {
        let mut subscription = self
            .repo
            .find_subscription(id)
            .await
            .map_err(internal)?
            .ok_or(NotificationServiceError::NotFound)?;
        if subscription.user_id() != actor.id() && actor.role() != Role::Owner {
            return Err(NotificationServiceError::Forbidden);
        }
        subscription.disable();
        self.repo
            .update_subscription(&subscription)
            .await
            .map_err(internal)?;
        self.audit_change(actor, id).await;
        Ok(())
    }

    /// Persist an event and create one stable delivery per matching subscription.
    pub async fn publish(
        &self,
        event: NotificationEvent,
    ) -> Result<Vec<Uuid>, NotificationServiceError> {
        self.repo.create_event(&event).await.map_err(internal)?;
        let subscriptions = self
            .repo
            .matching_subscriptions(event.kind())
            .await
            .map_err(internal)?;
        let mut created = Vec::new();
        for subscription in subscriptions
            .into_iter()
            .filter(|subscription| subscription.matches(&event))
        {
            let Some(channel) = self
                .repo
                .find_channel(subscription.channel_id())
                .await
                .map_err(internal)?
            else {
                continue;
            };
            if !channel.is_enabled()
                || self
                    .ensure_destination(&channel, subscription.destination())
                    .await
                    .is_err()
            {
                self.audit_delivery(
                    AuditAction::DeliveryRejected,
                    AuditOutcome::Denied,
                    Uuid::new_v4(),
                    channel.id(),
                )
                .await;
                continue;
            }
            let id = Uuid::new_v4();
            let body = event.webhook_body(id, self.max_payload_bytes)?;
            let delivery = DeliveryAttempt::pending(
                id,
                event.id(),
                channel.id(),
                subscription.id(),
                payload_digest(&body),
                Utc::now(),
            );
            if self
                .repo
                .create_delivery(&delivery)
                .await
                .map_err(internal)?
            {
                created.push(id);
            }
        }
        Ok(created)
    }

    /// Lease and process a bounded batch. Safe to call from tests and the task.
    pub async fn dispatch_once(
        &self,
        now: chrono::DateTime<Utc>,
        limit: u32,
    ) -> Result<usize, NotificationServiceError> {
        let deliveries = self
            .repo
            .lease_due(now, limit, Duration::seconds(30))
            .await
            .map_err(internal)?;
        let count = deliveries.len();
        for mut delivery in deliveries {
            let Some(event) = self
                .repo
                .find_event(delivery.event_id())
                .await
                .map_err(internal)?
            else {
                delivery.record_permanent_failure(now, "event missing")?;
                self.finish_failure(&delivery).await?;
                continue;
            };
            let Some(subscription) = self
                .repo
                .find_subscription(delivery.subscription_id())
                .await
                .map_err(internal)?
            else {
                delivery.record_permanent_failure(now, "subscription missing")?;
                self.finish_failure(&delivery).await?;
                continue;
            };
            let Some(channel) = self
                .repo
                .find_channel(delivery.channel_id())
                .await
                .map_err(internal)?
            else {
                delivery.record_permanent_failure(now, "channel missing")?;
                self.finish_failure(&delivery).await?;
                continue;
            };
            let outcome = if !channel.is_enabled()
                || !subscription.enabled()
                || self
                    .ensure_destination(&channel, subscription.destination())
                    .await
                    .is_err()
            {
                self.audit_delivery(
                    AuditAction::DeliveryRejected,
                    AuditOutcome::Denied,
                    delivery.id(),
                    channel.id(),
                )
                .await;
                AdapterOutcome::Permanent("destination rejected".into())
            } else if !self.take_rate_slot(channel.id(), now) {
                AdapterOutcome::Transient("channel rate limited".into())
            } else if let Some(config) = channel.smtp_config() {
                let secret = decrypt_from_storage(&self.master_key, config.password_enc())
                    .map_err(internal)?;
                self.adapter
                    .smtp(config, &secret, subscription.destination(), &event)
                    .await
            } else if let Some(config) = channel.webhook_config() {
                let secret = decrypt_from_storage(&self.master_key, config.secret_enc())
                    .map_err(internal)?;
                let body = event.webhook_body(delivery.id(), self.max_payload_bytes)?;
                if payload_digest(&body) != delivery.payload_digest() {
                    AdapterOutcome::Permanent("payload digest mismatch".into())
                } else {
                    self.adapter
                        .webhook(config, &secret, delivery.id(), &event, &body)
                        .await
                }
            } else {
                AdapterOutcome::Permanent("channel kind missing".into())
            };
            match outcome {
                AdapterOutcome::Accepted => {
                    delivery.record_success(now)?;
                    self.repo
                        .update_delivery(&delivery)
                        .await
                        .map_err(internal)?;
                    self.audit_delivery(
                        AuditAction::DeliverySucceeded,
                        AuditOutcome::Success,
                        delivery.id(),
                        channel.id(),
                    )
                    .await;
                }
                AdapterOutcome::Transient(error) => {
                    delivery.record_transient_failure(now, &error)?;
                    self.repo
                        .update_delivery(&delivery)
                        .await
                        .map_err(internal)?;
                    if delivery.status()
                        == openpanel_domain::notifications::DeliveryStatus::TerminalFailure
                    {
                        self.finish_failure(&delivery).await?;
                    }
                }
                AdapterOutcome::Permanent(error) => {
                    delivery.record_permanent_failure(now, &error)?;
                    self.finish_failure(&delivery).await?;
                }
            }
        }
        Ok(count)
    }

    /// Owner-only rolling health.
    pub async fn health(
        &self,
        actor: &User,
    ) -> Result<Vec<ChannelHealth>, NotificationServiceError> {
        owner(actor)?;
        let since = Utc::now() - Duration::hours(1);
        let mut values = Vec::new();
        for channel in self.repo.list_channels().await.map_err(internal)? {
            values.push(
                self.repo
                    .channel_health(channel.id(), since)
                    .await
                    .map_err(internal)?,
            );
        }
        Ok(values)
    }

    /// Owner-only delivery inspection with no payload or credentials.
    pub async fn delivery(
        &self,
        actor: &User,
        id: Uuid,
    ) -> Result<DeliveryView, NotificationServiceError> {
        owner(actor)?;
        let value = self
            .repo
            .find_delivery(id)
            .await
            .map_err(internal)?
            .ok_or(NotificationServiceError::NotFound)?;
        Ok(DeliveryView::from(&value))
    }

    async fn finish_failure(
        &self,
        delivery: &DeliveryAttempt,
    ) -> Result<(), NotificationServiceError> {
        self.repo
            .update_delivery(delivery)
            .await
            .map_err(internal)?;
        self.repo
            .increment_failure_count(delivery.subscription_id())
            .await
            .map_err(internal)?;
        self.audit_delivery(
            AuditAction::DeliveryFailed,
            AuditOutcome::Failure,
            delivery.id(),
            delivery.channel_id(),
        )
        .await;
        Ok(())
    }

    async fn ensure_destination(
        &self,
        channel: &Channel,
        destination: &str,
    ) -> Result<(), NotificationServiceError> {
        if let Some(config) = channel.smtp_config() {
            return if config.allows_recipient(destination) {
                Ok(())
            } else {
                Err(NotificationServiceError::DestinationRejected)
            };
        }
        let config = channel
            .webhook_config()
            .ok_or(NotificationServiceError::DestinationRejected)?;
        if destination != config.url() {
            return Err(NotificationServiceError::DestinationRejected);
        }
        let url = reqwest::Url::parse(destination)
            .map_err(|_| NotificationServiceError::DestinationRejected)?;
        let host = url
            .host_str()
            .ok_or(NotificationServiceError::DestinationRejected)?;
        let port = url
            .port_or_known_default()
            .ok_or(NotificationServiceError::DestinationRejected)?;
        let addresses = tokio::net::lookup_host((host, port))
            .await
            .map_err(|_| NotificationServiceError::DestinationRejected)?;
        if addresses
            .into_iter()
            .all(|address| !config.allows_ip(address.ip()))
        {
            return Err(NotificationServiceError::DestinationRejected);
        }
        Ok(())
    }

    async fn audit_change(&self, actor: &User, target: Uuid) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    actor.username().to_string(),
                    AuditAction::NotificationChanged,
                    AuditOutcome::Success,
                )
                .target(target.to_string()),
            )
            .await;
    }

    async fn audit_delivery(
        &self,
        action: AuditAction,
        outcome: AuditOutcome,
        delivery_id: Uuid,
        channel_id: Uuid,
    ) {
        let _ = self
            .audit
            .record(
                AuditEvent::new("notification-dispatcher", action, outcome)
                    .target(delivery_id.to_string())
                    .metadata(
                        serde_json::json!({"delivery_id":delivery_id,"channel_id":channel_id}),
                    ),
            )
            .await;
    }

    fn take_rate_slot(&self, channel_id: Uuid, now: chrono::DateTime<Utc>) -> bool {
        let Ok(mut calls) = self.channel_calls.lock() else {
            return false;
        };
        let calls = calls.entry(channel_id).or_default();
        while calls
            .front()
            .is_some_and(|at| *at <= now - Duration::minutes(1))
        {
            calls.pop_front();
        }
        if calls.len() >= self.calls_per_minute {
            return false;
        }
        calls.push_back(now);
        true
    }
}

/// Secret-free durable delivery response.
#[derive(Debug, Clone, Serialize)]
pub struct DeliveryView {
    /// Stable delivery identifier.
    pub id: Uuid,
    /// Event identifier.
    pub event_id: Uuid,
    /// Channel identifier.
    pub channel_id: Uuid,
    /// Subscription identifier.
    pub subscription_id: Uuid,
    /// Durable state.
    pub status: openpanel_domain::notifications::DeliveryStatus,
    /// Failed adapter call count.
    pub attempt_n: u32,
    /// Next retry if scheduled.
    pub next_retry_at: Option<chrono::DateTime<Utc>>,
    /// Redacted last adapter error.
    pub last_error: Option<String>,
}

impl From<&DeliveryAttempt> for DeliveryView {
    fn from(value: &DeliveryAttempt) -> Self {
        Self {
            id: value.id(),
            event_id: value.event_id(),
            channel_id: value.channel_id(),
            subscription_id: value.subscription_id(),
            status: value.status(),
            attempt_n: value.attempt_n(),
            next_retry_at: value.next_retry_at(),
            last_error: value.last_error_redacted().map(str::to_owned),
        }
    }
}

fn view(subscription: &Subscription) -> Result<SubscriptionView, NotificationServiceError> {
    Ok(SubscriptionView {
        id: subscription.id(),
        user_id: subscription.user_id(),
        channel_id: subscription.channel_id(),
        destination: subscription.destination().to_owned(),
        kind: subscription.kind(),
        filter: subscription.filter().to_json()?,
        enabled: subscription.enabled(),
    })
}
fn owner(actor: &User) -> Result<(), NotificationServiceError> {
    if actor.role() == Role::Owner {
        Ok(())
    } else {
        Err(NotificationServiceError::Forbidden)
    }
}
fn internal(error: impl std::fmt::Display) -> NotificationServiceError {
    NotificationServiceError::Internal(error.to_string())
}
