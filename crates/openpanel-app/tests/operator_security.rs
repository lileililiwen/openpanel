//! Operator security control-plane notification integration tests
//! over SQLite and an in-process adapter.
//!
//! Capability under test: `operator-security-control-plane`
//! (post-remediation verification publishes a redacted audit-family
//! notification; deliveries carry evidence without secrets).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use openpanel_app::{
    ControlRemediationKind, ControlRemediationPort, OperatorSecurityService,
    notifications::{
        AdapterOutcome, CreateSubscription, CreateWebhookChannel, NotificationAdapter,
        NotificationService, SqliteNotificationRepository,
    },
};
use openpanel_core::{Migration, MigrationRunner};
use openpanel_domain::{
    Role, User,
    common::{Email, Password, Username},
    notifications::{EventKind, NotificationEvent},
    operator_security::{FindingSeverity, FindingSource, RemediationMode, SecurityFinding},
};
use openpanel_test_support::{MockAudit, TestDb};
use uuid::Uuid;

struct FailVerifyPort;

#[async_trait]
impl ControlRemediationPort for FailVerifyPort {
    async fn execute(
        &self,
        _caller: &User,
        _kind: ControlRemediationKind,
        _finding: &SecurityFinding,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn verify(
        &self,
        _caller: &User,
        _kind: ControlRemediationKind,
        _finding: &SecurityFinding,
    ) -> Result<bool, String> {
        Ok(false)
    }

    async fn rollback(
        &self,
        _caller: &User,
        _kind: ControlRemediationKind,
        _finding: &SecurityFinding,
    ) -> Result<(), String> {
        Ok(())
    }
}

struct CaptureAdapter {
    bodies: Mutex<Vec<Vec<u8>>>,
}

#[async_trait]
impl NotificationAdapter for CaptureAdapter {
    async fn smtp(
        &self,
        _channel: &openpanel_domain::notifications::SmtpChannel,
        _password: &str,
        _recipient: &str,
        _event: &NotificationEvent,
    ) -> AdapterOutcome {
        panic!("SMTP adapter must not be selected")
    }

    async fn webhook(
        &self,
        _channel: &openpanel_domain::notifications::WebhookChannel,
        _secret: &str,
        _delivery_id: Uuid,
        _event: &NotificationEvent,
        body: &[u8],
    ) -> AdapterOutcome {
        self.bodies.lock().unwrap().push(body.to_vec());
        AdapterOutcome::Accepted
    }
}

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("control-plane-owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

/// A failed post-check publishes a critical audit-family notification
/// whose delivery carries redacted evidence.
#[tokio::test]
async fn operator_security_control_plane_failed_remediation_notifies_without_secrets() {
    let db = TestDb::new().await;
    let migration = Migration {
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
    let adapter = Arc::new(CaptureAdapter {
        bodies: Mutex::new(Vec::new()),
    });
    let notifications = Arc::new(NotificationService::new(
        repo.clone(),
        adapter.clone(),
        Arc::new(MockAudit::stub()),
        [7; 32],
    ));
    let owner = owner();
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
                kind: EventKind::Audit,
                filter: serde_json::json!({"actions": ["firewall_changed"]}),
            },
        )
        .await
        .unwrap();

    let control_plane =
        OperatorSecurityService::new(Arc::new(FailVerifyPort), Arc::new(MockAudit::stub()))
            .with_notifications(notifications.clone());
    let now = Utc::now();
    let finding = SecurityFinding::new(
        FindingSource::Firewall,
        FindingSeverity::High,
        "firewall:rules",
        "login-blocks-active",
        "Login-abuse blocks are active",
        "deny 203.0.113.7 password=hunter2",
        RemediationMode::Automatic,
        now,
    )
    .unwrap();
    control_plane
        .ingest(&owner, vec![finding.clone()], now)
        .await
        .unwrap();
    let failed = control_plane
        .remediate(&owner, finding.id(), "notify-key", true, now)
        .await
        .unwrap();
    assert_eq!(
        failed.state(),
        openpanel_domain::operator_security::FindingState::Failed
    );

    notifications.dispatch_once(now, 10).await.unwrap();
    let bodies = adapter.bodies.lock().unwrap();
    assert_eq!(bodies.len(), 1, "one audit delivery fanned out");
    let body = String::from_utf8_lossy(&bodies[0]);
    assert!(
        body.contains(&finding.id().to_string()),
        "delivery names the finding: {body}"
    );
    assert!(
        body.contains("firewall:rules"),
        "delivery keeps context: {body}"
    );
    assert!(!body.contains("hunter2"), "no secret: {body}");
    assert!(body.contains("audit.emitted"), "audit family event: {body}");
}
