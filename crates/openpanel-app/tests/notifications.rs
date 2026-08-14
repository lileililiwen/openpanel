//! Notification service integration tests over SQLite and an in-process adapter.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use chrono::{Duration, Utc};
use openpanel_app::notifications::{
    AdapterOutcome, CreateSmtpChannel, CreateSubscription, CreateWebhookChannel,
    NotificationAdapter, NotificationService, RustlsNotificationAdapter,
    SqliteNotificationRepository,
};
use openpanel_core::{AuditAction, AuditService, MigrationRunner, SqliteAuditService};
use openpanel_domain::{
    Role, User,
    common::{Email, Password, Username},
    notifications::{
        DeliveryStatus, EventKind, NotificationEvent, NotificationRepository, Severity,
        SmtpChannel, TlsMode, WebhookChannel,
    },
};
use openpanel_test_support::{MockAudit, TestDb};
use uuid::Uuid;

struct CaptureAdapter {
    outcomes: Mutex<VecDeque<AdapterOutcome>>,
    delivery_ids: Mutex<Vec<Uuid>>,
    bodies: Mutex<Vec<Vec<u8>>>,
}

impl CaptureAdapter {
    fn new(outcomes: Vec<AdapterOutcome>) -> Self {
        Self {
            outcomes: Mutex::new(outcomes.into()),
            delivery_ids: Mutex::new(Vec::new()),
            bodies: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl NotificationAdapter for CaptureAdapter {
    async fn smtp(
        &self,
        _channel: &SmtpChannel,
        _password: &str,
        _recipient: &str,
        _event: &NotificationEvent,
    ) -> AdapterOutcome {
        panic!("SMTP adapter must not be selected")
    }

    async fn webhook(
        &self,
        _channel: &WebhookChannel,
        secret: &str,
        delivery_id: Uuid,
        _event: &NotificationEvent,
        body: &[u8],
    ) -> AdapterOutcome {
        assert_eq!(secret, "test-signing-secret");
        self.delivery_ids.lock().unwrap().push(delivery_id);
        self.bodies.lock().unwrap().push(body.to_vec());
        self.outcomes
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(AdapterOutcome::Accepted)
    }
}

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("notification-owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

async fn fixture(
    outcomes: Vec<AdapterOutcome>,
) -> (
    TestDb,
    Arc<SqliteNotificationRepository>,
    Arc<CaptureAdapter>,
    Arc<NotificationService>,
    User,
) {
    let db = TestDb::new().await;
    let migration = openpanel_core::Migration {
        module: "notifications",
        version: "001".into(),
        description: "test".into(),
        sql: openpanel_app::migrations::NOTIFICATIONS_V001.into(),
    };
    MigrationRunner::for_sqlite(db.pool())
        .apply_module("notifications", &[migration])
        .await
        .unwrap();
    let repo = Arc::new(SqliteNotificationRepository::new(db.pool()));
    let adapter = Arc::new(CaptureAdapter::new(outcomes));
    let service = Arc::new(NotificationService::new(
        repo.clone(),
        adapter.clone(),
        Arc::new(MockAudit::stub()),
        [7; 32],
    ));
    (db, repo, adapter, service, owner())
}

#[tokio::test]
async fn alert_fanout_retries_then_reuses_delivery_id_and_exact_body() {
    let (_db, repo, adapter, service, owner) = fixture(vec![
        AdapterOutcome::Transient("timeout token=hidden".into()),
        AdapterOutcome::Accepted,
    ])
    .await;
    let channel = service
        .create_webhook(
            &owner,
            CreateWebhookChannel {
                name: "local receiver".into(),
                url: "https://127.0.0.1/hook".into(),
                signing_secret: "test-signing-secret".into(),
                allowlist: vec!["127.0.0.0/8".into()],
            },
        )
        .await
        .unwrap();
    service.create_subscription(&owner, CreateSubscription {
        channel_id: channel.id, destination: "https://127.0.0.1/hook".into(), kind: EventKind::Alert,
        filter: serde_json::json!({"metrics":["cpu_percent"],"severity_at_least":"warning"}),
    }).await.unwrap();
    let now = Utc::now();
    let ids = service
        .publish(
            NotificationEvent::new(
                Uuid::new_v4(),
                EventKind::Alert,
                "CPU high".into(),
                Severity::Critical,
                serde_json::json!({"metric":"cpu_percent","password":"not outbound"}),
                now,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ids.len(), 1);
    service.dispatch_once(now, 10).await.unwrap();
    let retry = repo.find_delivery(ids[0]).await.unwrap().unwrap();
    assert_eq!(retry.status(), DeliveryStatus::Retry);
    assert_eq!(retry.next_retry_at(), Some(now + Duration::minutes(1)));
    assert_eq!(
        retry.last_error_redacted(),
        Some("[REDACTED DELIVERY ERROR]")
    );
    service
        .dispatch_once(now + Duration::minutes(1), 10)
        .await
        .unwrap();
    let done = repo.find_delivery(ids[0]).await.unwrap().unwrap();
    assert_eq!(done.status(), DeliveryStatus::TerminalSuccess);
    let delivery_ids = adapter.delivery_ids.lock().unwrap();
    assert_eq!(delivery_ids.as_slice(), &[ids[0], ids[0]]);
    let bodies = adapter.bodies.lock().unwrap();
    assert_eq!(bodies[0], bodies[1]);
    assert!(!String::from_utf8_lossy(&bodies[0]).contains("not outbound"));
}

#[tokio::test]
async fn nonmatching_filter_creates_no_delivery_and_destination_is_enforced() {
    let (_db, _repo, adapter, service, owner) = fixture(vec![]).await;
    let channel = service
        .create_webhook(
            &owner,
            CreateWebhookChannel {
                name: "local receiver".into(),
                url: "https://127.0.0.1/hook".into(),
                signing_secret: "test-signing-secret".into(),
                allowlist: vec!["127.0.0.0/8".into()],
            },
        )
        .await
        .unwrap();
    assert!(
        service
            .create_subscription(
                &owner,
                CreateSubscription {
                    channel_id: channel.id,
                    destination: "https://127.0.0.2/hook".into(),
                    kind: EventKind::Alert,
                    filter: serde_json::json!({"metrics":["cpu_percent"]})
                }
            )
            .await
            .is_err()
    );
    service
        .create_subscription(
            &owner,
            CreateSubscription {
                channel_id: channel.id,
                destination: "https://127.0.0.1/hook".into(),
                kind: EventKind::Alert,
                filter: serde_json::json!({"metrics":["memory_percent"]}),
            },
        )
        .await
        .unwrap();
    let ids = service
        .publish(
            NotificationEvent::new(
                Uuid::new_v4(),
                EventKind::Alert,
                "CPU high".into(),
                Severity::Warning,
                serde_json::json!({"metric":"cpu_percent"}),
                Utc::now(),
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert!(ids.is_empty());
    assert!(adapter.delivery_ids.lock().unwrap().is_empty());
}

struct UnusedCollector;

impl openpanel_app::monitoring::Collector for UnusedCollector {
    fn snapshot(
        &mut self,
    ) -> Result<
        openpanel_domain::monitoring::SystemSnapshot,
        openpanel_domain::monitoring::MonitoringError,
    > {
        unreachable!("evaluate_alerts does not collect")
    }
}

#[tokio::test]
async fn monitoring_alert_publishes_to_matching_subscription() {
    let (_db, repo, _adapter, notifications, owner) = fixture(vec![]).await;
    let channel = notifications
        .create_webhook(
            &owner,
            CreateWebhookChannel {
                name: "local receiver".into(),
                url: "https://127.0.0.1/hook".into(),
                signing_secret: "test-signing-secret".into(),
                allowlist: vec!["127.0.0.0/8".into()],
            },
        )
        .await
        .unwrap();
    notifications
        .create_subscription(
            &owner,
            CreateSubscription {
                channel_id: channel.id,
                destination: "https://127.0.0.1/hook".into(),
                kind: EventKind::Alert,
                filter: serde_json::json!({"metrics":["cpu_percent"],"severity_at_least":"warning"}),
            },
        )
        .await
        .unwrap();
    let monitoring = openpanel_app::MonitoringService::new(
        Arc::new(openpanel_test_support::MockSnapshotRepo::new()),
        Arc::new(Mutex::new(Box::new(UnusedCollector))),
        Arc::new(MockAudit::stub()),
        openpanel_app::monitoring::AlertEvaluator::new(&openpanel_app::monitoring::AlertConfig {
            cpu_percent: Some(50.0),
            ..Default::default()
        }),
        7,
    );
    monitoring.attach_notifications(notifications);
    let snapshot = openpanel_domain::monitoring::SystemSnapshot::new(
        Utc::now(),
        0.5,
        90.0,
        40.0,
        vec![openpanel_domain::monitoring::DiskReading {
            mount: "/".into(),
            percent: 10.0,
        }],
        vec![],
    )
    .unwrap();
    assert_eq!(
        monitoring.evaluate_alerts(&snapshot).await.unwrap().len(),
        1
    );
    assert_eq!(
        repo.lease_due(Utc::now(), 10, Duration::seconds(30))
            .await
            .unwrap()
            .len(),
        1
    );
}

async fn smtp_capture() -> (u16, tokio::sync::oneshot::Receiver<Vec<u8>>) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let (body_tx, body_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        writer.write_all(b"220 capture ESMTP\r\n").await.unwrap();
        let mut data = false;
        let mut body = Vec::new();
        while let Some(line) = lines.next_line().await.unwrap() {
            if data {
                if line == "." {
                    writer.write_all(b"250 queued\r\n").await.unwrap();
                    let _ = body_tx.send(body);
                    break;
                }
                body.extend_from_slice(line.as_bytes());
                body.extend_from_slice(b"\r\n");
                continue;
            }
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("EHLO") {
                writer
                    .write_all(b"250-capture\r\n250 AUTH PLAIN LOGIN\r\n")
                    .await
                    .unwrap();
            } else if upper.starts_with("AUTH") {
                writer.write_all(b"235 authenticated\r\n").await.unwrap();
            } else if upper.starts_with("MAIL FROM") || upper.starts_with("RCPT TO") {
                writer.write_all(b"250 ok\r\n").await.unwrap();
            } else if upper == "DATA" {
                data = true;
                writer.write_all(b"354 end with dot\r\n").await.unwrap();
            } else {
                writer.write_all(b"250 ok\r\n").await.unwrap();
            }
        }
    });
    (port, body_rx)
}

#[tokio::test]
#[ignore = "requires loopback socket capability"]
async fn cpu_alert_reaches_real_smtp_capture_and_records_success_audit() {
    let (port, body_rx) = smtp_capture().await;
    let db = TestDb::new().await;
    let migration = openpanel_core::Migration {
        module: "notifications",
        version: "001".into(),
        description: "test".into(),
        sql: openpanel_app::migrations::NOTIFICATIONS_V001.into(),
    };
    MigrationRunner::for_sqlite(db.pool())
        .apply_module("notifications", &[migration])
        .await
        .unwrap();
    let repo = Arc::new(SqliteNotificationRepository::new(db.pool()));
    let audit = Arc::new(SqliteAuditService::new(db.pool()));
    let service = NotificationService::new(
        repo,
        Arc::new(RustlsNotificationAdapter::new().unwrap()),
        audit.clone(),
        [9; 32],
    );
    let owner = owner();
    let channel = service
        .create_smtp(
            &owner,
            CreateSmtpChannel {
                name: "capture".into(),
                host: "127.0.0.1".into(),
                port,
                username: "mailer".into(),
                password: "smtp-password".into(),
                from_addr: "sender@example.test".into(),
                tls_mode: TlsMode::None,
                allowlist: vec!["ops@example.test".into()],
            },
        )
        .await
        .unwrap();
    service.create_subscription(&owner, CreateSubscription {
        channel_id: channel.id, destination: "ops@example.test".into(), kind: EventKind::Alert,
        filter: serde_json::json!({"metrics":["cpu_percent"],"severity_at_least":"warning"}),
    }).await.unwrap();
    let now = Utc::now();
    service
        .publish(
            NotificationEvent::new(
                Uuid::new_v4(),
                EventKind::Alert,
                "CPU threshold crossed".into(),
                Severity::Warning,
                serde_json::json!({"metric":"cpu_percent","value":95}),
                now,
            )
            .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(service.dispatch_once(now, 10).await.unwrap(), 1);
    let body = tokio::time::timeout(std::time::Duration::from_secs(2), body_rx)
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&body).contains("CPU threshold crossed"));
    assert!(
        audit
            .recent(20)
            .await
            .unwrap()
            .iter()
            .any(|event| event.action == AuditAction::DeliverySucceeded)
    );
}
