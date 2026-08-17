//! Webmail client integration tests: session lifecycle, message
//! redaction, and quota / sending-policy enforcement.

use std::sync::Arc;

use openpanel_app::webmail_client::InMemoryMailBridge;
use openpanel_domain::{
    MailBridge, MessageBody, WebmailError,
    mail::{DomainSendingPolicy, MailboxQuota},
};

use crate::common::*;

#[tokio::test]
async fn session_mint_validate_expire() {
    let server = TestServer::new().await;
    let svc = server.webmail();

    let session = svc
        .mint_session("webmail", "alice@example.com")
        .await
        .expect("mint");
    assert_eq!(session.mailbox(), "alice@example.com");
    assert!(!session.token().is_empty());

    // Valid within the 15-minute window.
    let valid = svc.validate_session(session.token()).await.expect("valid");
    assert_eq!(valid.id(), session.id());

    // Expired tokens are revoked and rejected.
    let repo = svc.repo().clone();
    let expired = openpanel_domain::WebmailSessionToken::new(
        uuid::Uuid::new_v4(),
        "bob@example.com",
        "expired-token",
        "cipher",
        "orig",
        chrono::Utc::now() - chrono::Duration::minutes(30),
    )
    .expect("expired token");
    repo.insert_session(&expired).await.expect("insert");
    let err = svc
        .validate_session("expired-token")
        .await
        .expect_err("expired must be rejected");
    assert!(matches!(err, WebmailError::InvalidSession));
}

#[tokio::test]
async fn redact_html_strips_scripts_and_tracking_pixels() {
    let html = concat!(
        "<div><script>alert(1)</script><p>Hello</p>",
        "<img src=\"https://tracker.example/p.png\" width=\"1\" height=\"1\" alt=\"p\"></div>"
    );
    let redacted = openpanel_app::redact_html(html);
    assert!(!redacted.contains("<script"));
    assert!(!redacted.contains("src=\"https://tracker.example/p.png\""));
    assert!(redacted.contains("Hello"));
    // The img element survives (screen readers) but without src.
    assert!(redacted.contains("<img"));
    assert!(!redacted.contains("src="));
}

#[tokio::test]
async fn quota_and_policy_enforced() {
    let bridge = InMemoryMailBridge::default();
    let session = openpanel_domain::WebmailSessionToken::new(
        uuid::Uuid::new_v4(),
        "alice@example.com",
        "tok",
        "cipher",
        "orig",
        chrono::Utc::now(),
    )
    .expect("session");
    let quota = MailboxQuota::new(100, 1, 1_000_000).expect("quota");
    let policy = DomainSendingPolicy::default_for("example.com");

    // Over-quota refused.
    let over = bridge
        .send(
            "alice@example.com",
            "bob@example.com",
            "hi",
            &"x".repeat(200),
            "",
            &policy,
        )
        .await;
    assert!(over.is_ok()); // in-memory bridge has no quota; the
    // service layer enforces it below.

    // Service-level quota check.
    let svc = openpanel_app::WebmailService::new(
        Arc::new(openpanel_app::webmail_client::SqliteWebmailRepository::new(
            sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .expect("pool"),
        )),
        Arc::new(bridge),
        Arc::new(openpanel_test_support::MockAudit::stub()),
        [0u8; 32],
    );
    let err = svc
        .send(
            &session,
            "bob@example.com",
            "hi",
            &"x".repeat(200),
            "",
            quota,
            0,
            policy,
        )
        .await
        .expect_err("over quota");
    assert!(matches!(err, WebmailError::MailboxQuotaExceeded));

    // Recipient-cap policy violation refused.
    let strict = openpanel_domain::mail::DomainSendingPolicy::restore(
        "example.com".to_string(),
        60,
        1,
        openpanel_domain::mail::SendingRequirement::Required,
        openpanel_domain::mail::SendingRequirement::Required,
        openpanel_domain::mail::SendingRequirement::SoftFail,
    );
    let err = svc
        .send(
            &session,
            "a@example.com,b@example.com",
            "hi",
            "body",
            "body",
            quota,
            0,
            strict,
        )
        .await
        .expect_err("recipient cap");
    assert!(matches!(
        err,
        WebmailError::SendingPolicyViolation(ref r) if r == "recipient_cap"
    ));
}

#[tokio::test]
async fn bridge_message_roundtrip() {
    let bridge = InMemoryMailBridge::default();
    bridge.seed(
        "alice@example.com",
        MessageBody {
            uid: 1,
            subject: "Hello".to_string(),
            from: "bob@example.com".to_string(),
            to: "alice@example.com".to_string(),
            html: "<p>Hi</p>".to_string(),
            text: "Hi".to_string(),
            date: chrono::Utc::now(),
        },
    );
    let folders = bridge
        .list_folders("alice@example.com")
        .await
        .expect("folders");
    assert!(folders.contains(&"INBOX".to_string()));
    let msgs = bridge
        .list_messages("alice@example.com", "INBOX", 10)
        .await
        .expect("messages");
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].subject, "Hello");
}
