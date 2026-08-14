//! SQLite persistence for durable notification delivery.

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use openpanel_domain::{
    RepoError,
    notifications::{
        Channel, ChannelHealth, DeliveryAttempt, DeliveryStatus, EventFilter, EventKind,
        NotificationEvent, NotificationRepository, Severity, Subscription, TlsMode,
    },
};
use sqlx::{Pool, Row, Sqlite};
use uuid::Uuid;

/// SQLite notification repository.
pub struct SqliteNotificationRepository {
    pool: Pool<Sqlite>,
}

impl SqliteNotificationRepository {
    /// Construct over an initialized pool.
    pub fn new(pool: Pool<Sqlite>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl NotificationRepository for SqliteNotificationRepository {
    async fn create_channel(&self, channel: &Channel) -> Result<(), RepoError> {
        let metadata = channel.metadata();
        let (endpoint, port, username, credential, from_addr, tls_mode, allowlist) =
            if let Some(config) = channel.smtp_config() {
                (
                    config.host().to_owned(),
                    Some(i64::from(config.port())),
                    Some(config.username().to_owned()),
                    config.password_enc().to_owned(),
                    Some(config.from_addr().to_owned()),
                    Some(tls_name(config.tls_mode()).to_owned()),
                    config.allowlist().iter().cloned().collect::<Vec<_>>(),
                )
            } else if let Some(config) = channel.webhook_config() {
                (
                    config.url().to_owned(),
                    None,
                    None,
                    config.secret_enc().to_owned(),
                    None,
                    None,
                    config.allowlist().iter().cloned().collect::<Vec<_>>(),
                )
            } else {
                return Err(RepoError::new("unknown notification channel"));
            };
        sqlx::query("INSERT INTO notification_channels(id,name,kind,endpoint,port,username,credential_enc,from_addr,tls_mode,allowlist_json,created_at,disabled_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(channel.id().to_string()).bind(channel.name()).bind(channel.kind().to_string())
            .bind(endpoint).bind(port).bind(username).bind(credential).bind(from_addr).bind(tls_mode)
            .bind(serde_json::to_string(&allowlist).map_err(repo_error)?)
            .bind(channel.created_at().to_rfc3339()).bind(metadata.disabled_at.map(|v| v.to_rfc3339()))
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn update_channel(&self, channel: &Channel) -> Result<(), RepoError> {
        let disabled = channel.disabled_at().map(|value| value.to_rfc3339());
        let credential = channel
            .smtp_config()
            .map(|c| c.password_enc())
            .or_else(|| channel.webhook_config().map(|c| c.secret_enc()));
        sqlx::query(
            "UPDATE notification_channels SET name=?,credential_enc=?,disabled_at=? WHERE id=?",
        )
        .bind(channel.name())
        .bind(credential)
        .bind(disabled)
        .bind(channel.id().to_string())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn find_channel(&self, id: Uuid) -> Result<Option<Channel>, RepoError> {
        sqlx::query("SELECT * FROM notification_channels WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(row_channel)
            .transpose()
    }

    async fn list_channels(&self) -> Result<Vec<Channel>, RepoError> {
        sqlx::query("SELECT * FROM notification_channels ORDER BY created_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(repo_error)?
            .into_iter()
            .map(row_channel)
            .collect()
    }

    async fn create_subscription(&self, subscription: &Subscription) -> Result<(), RepoError> {
        sqlx::query("INSERT INTO notification_subscriptions(id,user_id,channel_id,destination,kind,filter_json,enabled,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(subscription.id().to_string()).bind(subscription.user_id().to_string())
            .bind(subscription.channel_id().to_string()).bind(subscription.destination())
            .bind(kind_name(subscription.kind()))
            .bind(subscription.filter().to_json().map_err(repo_error)?.to_string())
            .bind(subscription.enabled()).bind(subscription.created_at().to_rfc3339())
            .execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn update_subscription(&self, subscription: &Subscription) -> Result<(), RepoError> {
        sqlx::query("UPDATE notification_subscriptions SET enabled=? WHERE id=? AND user_id=?")
            .bind(subscription.enabled())
            .bind(subscription.id().to_string())
            .bind(subscription.user_id().to_string())
            .execute(&self.pool)
            .await
            .map_err(repo_error)?;
        Ok(())
    }

    async fn find_subscription(&self, id: Uuid) -> Result<Option<Subscription>, RepoError> {
        sqlx::query("SELECT * FROM notification_subscriptions WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(row_subscription)
            .transpose()
    }

    async fn list_subscriptions(&self, user_id: Uuid) -> Result<Vec<Subscription>, RepoError> {
        sqlx::query(
            "SELECT * FROM notification_subscriptions WHERE user_id=? ORDER BY created_at DESC",
        )
        .bind(user_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(repo_error)?
        .into_iter()
        .map(row_subscription)
        .collect()
    }

    async fn matching_subscriptions(
        &self,
        kind: EventKind,
    ) -> Result<Vec<Subscription>, RepoError> {
        sqlx::query("SELECT * FROM notification_subscriptions WHERE kind=? AND enabled=1")
            .bind(kind_name(kind))
            .fetch_all(&self.pool)
            .await
            .map_err(repo_error)?
            .into_iter()
            .map(row_subscription)
            .collect()
    }

    async fn create_event(&self, event: &NotificationEvent) -> Result<(), RepoError> {
        sqlx::query("INSERT OR IGNORE INTO notification_events(id,kind,subject,severity,details_json,occurred_at) VALUES(?,?,?,?,?,?)")
            .bind(event.id().to_string()).bind(kind_name(event.kind())).bind(event.subject())
            .bind(severity_name(event.severity())).bind(event.details().to_string())
            .bind(event.occurred_at().to_rfc3339()).execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn find_event(&self, id: Uuid) -> Result<Option<NotificationEvent>, RepoError> {
        let row = sqlx::query("SELECT * FROM notification_events WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?;
        row.map(|row| {
            NotificationEvent::new(
                parse_uuid(row.get::<String, _>("id"))?,
                parse_kind(row.get("kind"))?,
                row.get("subject"),
                parse_severity(row.get("severity"))?,
                serde_json::from_str(row.get("details_json")).map_err(repo_error)?,
                parse_time(row.get("occurred_at"))?,
            )
            .map_err(repo_error)
        })
        .transpose()
    }

    async fn create_delivery(&self, delivery: &DeliveryAttempt) -> Result<bool, RepoError> {
        let result = sqlx::query("INSERT OR IGNORE INTO notification_deliveries(id,event_id,channel_id,subscription_id,payload_digest,status,attempt_n,next_retry_at,lease_until,last_error_redacted,created_at,completed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(delivery.id().to_string()).bind(delivery.event_id().to_string())
            .bind(delivery.channel_id().to_string()).bind(delivery.subscription_id().to_string())
            .bind(delivery.payload_digest()).bind(status_name(delivery.status())).bind(i64::from(delivery.attempt_n()))
            .bind(time_opt(delivery.next_retry_at())).bind(time_opt(delivery.lease_until()))
            .bind(delivery.last_error_redacted()).bind(delivery.created_at().to_rfc3339())
            .bind(time_opt(delivery.completed_at())).execute(&self.pool).await.map_err(repo_error)?;
        Ok(result.rows_affected() == 1)
    }

    async fn lease_due(
        &self,
        now: DateTime<Utc>,
        limit: u32,
        lease_for: Duration,
    ) -> Result<Vec<DeliveryAttempt>, RepoError> {
        let mut tx = self.pool.begin().await.map_err(repo_error)?;
        sqlx::query("UPDATE notification_deliveries SET status='pending',lease_until=NULL WHERE status='leased' AND lease_until<?")
            .bind(now.to_rfc3339()).execute(&mut *tx).await.map_err(repo_error)?;
        let rows = sqlx::query("SELECT id FROM notification_deliveries WHERE status='pending' OR (status='retry' AND next_retry_at<=?) ORDER BY created_at LIMIT ?")
            .bind(now.to_rfc3339()).bind(i64::from(limit)).fetch_all(&mut *tx).await.map_err(repo_error)?;
        let mut ids = Vec::new();
        for row in rows {
            let id: String = row.get("id");
            let result = sqlx::query("UPDATE notification_deliveries SET status='leased',lease_until=? WHERE id=? AND (status='pending' OR (status='retry' AND next_retry_at<=?))")
                .bind((now + lease_for).to_rfc3339()).bind(&id).bind(now.to_rfc3339())
                .execute(&mut *tx).await.map_err(repo_error)?;
            if result.rows_affected() == 1 {
                ids.push(id);
            }
        }
        tx.commit().await.map_err(repo_error)?;
        let mut deliveries = Vec::new();
        for id in ids {
            let row = sqlx::query("SELECT * FROM notification_deliveries WHERE id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(repo_error)?;
            deliveries.push(row_delivery(row)?);
        }
        Ok(deliveries)
    }

    async fn update_delivery(&self, delivery: &DeliveryAttempt) -> Result<(), RepoError> {
        sqlx::query("UPDATE notification_deliveries SET status=?,attempt_n=?,next_retry_at=?,lease_until=?,last_error_redacted=?,completed_at=? WHERE id=?")
            .bind(status_name(delivery.status())).bind(i64::from(delivery.attempt_n()))
            .bind(time_opt(delivery.next_retry_at())).bind(time_opt(delivery.lease_until()))
            .bind(delivery.last_error_redacted()).bind(time_opt(delivery.completed_at()))
            .bind(delivery.id().to_string()).execute(&self.pool).await.map_err(repo_error)?;
        Ok(())
    }

    async fn find_delivery(&self, id: Uuid) -> Result<Option<DeliveryAttempt>, RepoError> {
        sqlx::query("SELECT * FROM notification_deliveries WHERE id=?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(repo_error)?
            .map(row_delivery)
            .transpose()
    }

    async fn increment_failure_count(&self, subscription_id: Uuid) -> Result<(), RepoError> {
        sqlx::query(
            "UPDATE notification_subscriptions SET failure_count=failure_count+1 WHERE id=?",
        )
        .bind(subscription_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(repo_error)?;
        Ok(())
    }

    async fn channel_health(
        &self,
        channel_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<ChannelHealth, RepoError> {
        let row = sqlx::query("SELECT COALESCE(SUM(CASE WHEN status='terminal_success' AND completed_at>=? THEN 1 ELSE 0 END),0) delivered, COALESCE(SUM(CASE WHEN status='terminal_failure' AND completed_at>=? THEN 1 ELSE 0 END),0) failed, COALESCE(SUM(CASE WHEN status IN ('pending','retry','leased') THEN 1 ELSE 0 END),0) pending, MIN(CASE WHEN status IN ('pending','retry','leased') THEN created_at END) oldest_pending, MIN(CASE WHEN status='retry' THEN next_retry_at END) next_retry FROM notification_deliveries WHERE channel_id=?")
            .bind(since.to_rfc3339()).bind(since.to_rfc3339()).bind(channel_id.to_string())
            .fetch_one(&self.pool).await.map_err(repo_error)?;
        let subscriptions: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM notification_subscriptions WHERE channel_id=? AND enabled=1",
        )
        .bind(channel_id.to_string())
        .fetch_one(&self.pool)
        .await
        .map_err(repo_error)?;
        let delivered = row.get::<i64, _>("delivered") as u64;
        Ok(ChannelHealth {
            channel_id,
            delivered,
            failed: row.get::<i64, _>("failed") as u64,
            pending: row.get::<i64, _>("pending") as u64,
            oldest_pending_at: parse_time_opt(row.get("oldest_pending"))?,
            next_retry_at: parse_time_opt(row.get("next_retry"))?,
            degraded: subscriptions > 0 && delivered == 0,
        })
    }
}

fn row_channel(row: sqlx::sqlite::SqliteRow) -> Result<Channel, RepoError> {
    let id = parse_uuid(row.get("id"))?;
    let created = parse_time(row.get("created_at"))?;
    let allowlist: Vec<String> =
        serde_json::from_str(row.get("allowlist_json")).map_err(repo_error)?;
    let channel = match row.get::<String, _>("kind").as_str() {
        "smtp" => Channel::smtp(
            id,
            row.get("name"),
            row.get("endpoint"),
            row.get::<i64, _>("port") as u16,
            row.get("username"),
            row.get("credential_enc"),
            row.get("from_addr"),
            parse_tls(row.get("tls_mode"))?,
            allowlist,
            created,
        ),
        "webhook" => Channel::webhook(
            id,
            row.get("name"),
            row.get("endpoint"),
            row.get("credential_enc"),
            allowlist,
            created,
        ),
        _ => return Err(RepoError::new("invalid channel kind")),
    }
    .map_err(repo_error)?;
    Ok(channel.restore_disabled_at(parse_time_opt(row.get("disabled_at"))?))
}

fn row_subscription(row: sqlx::sqlite::SqliteRow) -> Result<Subscription, RepoError> {
    let kind = parse_kind(row.get("kind"))?;
    let filter = EventFilter::from_json(
        kind,
        serde_json::from_str(row.get("filter_json")).map_err(repo_error)?,
    )
    .map_err(repo_error)?;
    Subscription::new(
        parse_uuid(row.get("id"))?,
        parse_uuid(row.get("user_id"))?,
        parse_uuid(row.get("channel_id"))?,
        row.get("destination"),
        kind,
        filter,
        parse_time(row.get("created_at"))?,
    )
    .map(|value| value.restore_enabled(row.get::<bool, _>("enabled")))
    .map_err(repo_error)
}

fn row_delivery(row: sqlx::sqlite::SqliteRow) -> Result<DeliveryAttempt, RepoError> {
    DeliveryAttempt::restore(
        parse_uuid(row.get("id"))?,
        parse_uuid(row.get("event_id"))?,
        parse_uuid(row.get("channel_id"))?,
        parse_uuid(row.get("subscription_id"))?,
        row.get("payload_digest"),
        parse_status(row.get("status"))?,
        row.get::<i64, _>("attempt_n") as u32,
        parse_time_opt(row.get("next_retry_at"))?,
        parse_time_opt(row.get("lease_until"))?,
        row.get("last_error_redacted"),
        parse_time(row.get("created_at"))?,
        parse_time_opt(row.get("completed_at"))?,
    )
    .map_err(repo_error)
}

fn kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::Alert => "alert",
        EventKind::Audit => "audit",
        EventKind::JobTerminal => "job_terminal",
    }
}
fn parse_kind(value: String) -> Result<EventKind, RepoError> {
    match value.as_str() {
        "alert" => Ok(EventKind::Alert),
        "audit" => Ok(EventKind::Audit),
        "job_terminal" => Ok(EventKind::JobTerminal),
        _ => Err(RepoError::new("invalid event kind")),
    }
}
fn severity_name(value: Severity) -> &'static str {
    match value {
        Severity::Info => "info",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    }
}
fn parse_severity(value: String) -> Result<Severity, RepoError> {
    match value.as_str() {
        "info" => Ok(Severity::Info),
        "warning" => Ok(Severity::Warning),
        "critical" => Ok(Severity::Critical),
        _ => Err(RepoError::new("invalid severity")),
    }
}
fn status_name(value: DeliveryStatus) -> &'static str {
    match value {
        DeliveryStatus::Pending => "pending",
        DeliveryStatus::Leased => "leased",
        DeliveryStatus::Retry => "retry",
        DeliveryStatus::TerminalSuccess => "terminal_success",
        DeliveryStatus::TerminalFailure => "terminal_failure",
    }
}
fn parse_status(value: String) -> Result<DeliveryStatus, RepoError> {
    match value.as_str() {
        "pending" => Ok(DeliveryStatus::Pending),
        "leased" => Ok(DeliveryStatus::Leased),
        "retry" => Ok(DeliveryStatus::Retry),
        "terminal_success" => Ok(DeliveryStatus::TerminalSuccess),
        "terminal_failure" => Ok(DeliveryStatus::TerminalFailure),
        _ => Err(RepoError::new("invalid delivery status")),
    }
}
fn tls_name(value: TlsMode) -> &'static str {
    match value {
        TlsMode::Tls => "tls",
        TlsMode::StartTls => "start_tls",
        TlsMode::None => "none",
    }
}
fn parse_tls(value: String) -> Result<TlsMode, RepoError> {
    match value.as_str() {
        "tls" => Ok(TlsMode::Tls),
        "start_tls" => Ok(TlsMode::StartTls),
        "none" => Ok(TlsMode::None),
        _ => Err(RepoError::new("invalid TLS mode")),
    }
}
fn parse_uuid(value: String) -> Result<Uuid, RepoError> {
    Uuid::parse_str(&value).map_err(repo_error)
}
fn parse_time(value: String) -> Result<DateTime<Utc>, RepoError> {
    Ok(DateTime::parse_from_rfc3339(&value)
        .map_err(repo_error)?
        .with_timezone(&Utc))
}
fn parse_time_opt(value: Option<String>) -> Result<Option<DateTime<Utc>>, RepoError> {
    value.map(parse_time).transpose()
}
fn time_opt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(|time| time.to_rfc3339())
}
fn repo_error(error: impl std::fmt::Display) -> RepoError {
    RepoError::new(error.to_string())
}
