//! Rustls-only SMTP and HTTPS notification transports.

use async_trait::async_trait;
use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::Mailbox,
    transport::smtp::{authentication::Credentials, client::Tls},
};
use openpanel_domain::notifications::{
    NotificationEvent, SmtpChannel, TlsMode, WebhookChannel, sign_webhook,
};
use uuid::Uuid;

/// Adapter classification controls retry behavior.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterOutcome {
    /// Remote accepted the message.
    Accepted,
    /// Retryable transport or 4xx failure.
    Transient(String),
    /// Permanent 5xx SMTP or non-retryable webhook failure.
    Permanent(String),
}

/// Typed outbound transport boundary.
#[async_trait]
pub trait NotificationAdapter: Send + Sync + 'static {
    /// Deliver one email without logging its body.
    async fn smtp(
        &self,
        channel: &SmtpChannel,
        password: &str,
        recipient: &str,
        event: &NotificationEvent,
    ) -> AdapterOutcome;
    /// Deliver an exact signed JSON body.
    async fn webhook(
        &self,
        channel: &WebhookChannel,
        secret: &str,
        delivery_id: Uuid,
        event: &NotificationEvent,
        body: &[u8],
    ) -> AdapterOutcome;
}

/// Production SMTP and reqwest transport.
pub struct RustlsNotificationAdapter {
    client: reqwest::Client,
}

impl RustlsNotificationAdapter {
    /// Build with bounded redirects and request timeout.
    pub fn new() -> Result<Self, String> {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(20))
            .build()
            .map(|client| Self { client })
            .map_err(|error| error.to_string())
    }
}

#[async_trait]
impl NotificationAdapter for RustlsNotificationAdapter {
    async fn smtp(
        &self,
        channel: &SmtpChannel,
        password: &str,
        recipient: &str,
        event: &NotificationEvent,
    ) -> AdapterOutcome {
        let from = match channel.from_addr().parse::<Mailbox>() {
            Ok(value) => value,
            Err(_) => return AdapterOutcome::Permanent("invalid sender".into()),
        };
        let to = match recipient.parse::<Mailbox>() {
            Ok(value) => value,
            Err(_) => return AdapterOutcome::Permanent("invalid recipient".into()),
        };
        let message = match Message::builder()
            .from(from)
            .to(to)
            .subject(event.subject())
            .body(event.details().to_string())
        {
            Ok(value) => value,
            Err(error) => return AdapterOutcome::Permanent(error.to_string()),
        };
        let credentials = Credentials::new(channel.username().to_owned(), password.to_owned());
        let builder = match channel.tls_mode() {
            TlsMode::Tls => AsyncSmtpTransport::<Tokio1Executor>::relay(channel.host()),
            TlsMode::StartTls => {
                AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(channel.host())
            }
            TlsMode::None => Ok(AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(
                channel.host(),
            )
            .tls(Tls::None)),
        };
        let transport = match builder {
            Ok(value) => value.port(channel.port()).credentials(credentials).build(),
            Err(error) => return AdapterOutcome::Permanent(error.to_string()),
        };
        match transport.send(message).await {
            Ok(_) => AdapterOutcome::Accepted,
            Err(error) if error.is_transient() => AdapterOutcome::Transient(error.to_string()),
            Err(error) => AdapterOutcome::Permanent(error.to_string()),
        }
    }

    async fn webhook(
        &self,
        channel: &WebhookChannel,
        secret: &str,
        delivery_id: Uuid,
        event: &NotificationEvent,
        body: &[u8],
    ) -> AdapterOutcome {
        let response = self
            .client
            .post(channel.url())
            .header("content-type", "application/json")
            .header(
                "x-openpanel-signature",
                sign_webhook(secret.as_bytes(), body),
            )
            .header("x-openpanel-delivery", delivery_id.to_string())
            .header("x-openpanel-event", event.kind().as_str())
            .body(body.to_vec())
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => AdapterOutcome::Accepted,
            Ok(response)
                if response.status().is_server_error()
                    || response.status().as_u16() == 408
                    || response.status().as_u16() == 429 =>
            {
                AdapterOutcome::Transient(format!("HTTP {}", response.status().as_u16()))
            }
            Ok(response) => {
                AdapterOutcome::Permanent(format!("HTTP {}", response.status().as_u16()))
            }
            Err(error) if error.is_timeout() || error.is_connect() => {
                AdapterOutcome::Transient(error.to_string())
            }
            Err(error) => AdapterOutcome::Permanent(error.to_string()),
        }
    }
}
