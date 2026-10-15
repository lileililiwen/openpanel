//! Staging mail-capture: capture port, encrypted-at-rest inbox,
//! FIFO retention, toggle default-off, destroy-purges-cascade, and
//! webmail divert-vs-passthrough.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{Duration, Utc};
use openpanel_app::{
    InMemoryStagingFilesystem, MailCaptureService,
    mail_capture::SqliteMailCaptureRepository,
    migrations::{
        IDENTITY_V001, MAIL_CAPTURE_V001, SITE_STAGING_V001, SITE_STAGING_V002, SITES_V001,
    },
    site_staging::{
        CreateSlotRequest, StagingService,
        repo::{
            SqlitePromotionRepository, SqliteStagingLockTable, SqliteStagingSlotRepository,
            SqliteStagingSnapshotRepository,
        },
    },
    sites::repo::SqliteSiteRepository,
    webmail_client::{InMemoryMailBridge, SqliteWebmailRepository},
};
use openpanel_domain::{
    CapturedMessage, Email, MailBridge, MailCaptureError, MailCapturePort, Password, Role, Site,
    User, Username, WebmailSessionToken,
    mail::{DomainSendingPolicy, MailboxQuota},
};
use openpanel_test_support::MockAudit;
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use uuid::Uuid;

const DOMAIN: &str = "cap.example.test";
const PLAINTEXT: &str = "secret-body-7f3c-must-never-hit-disk";

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

fn site(owner_id: Uuid) -> Site {
    Site::new(
        Uuid::new_v4(),
        owner_id,
        DOMAIN,
        vec![],
        format!("/srv/openpanel/sites/{DOMAIN}/public"),
        false,
        None,
        "owner",
    )
    .unwrap()
}

struct Harness {
    pool: Pool<Sqlite>,
    owner: User,
    site: Site,
    slot_id: Uuid,
    staging: Arc<StagingService>,
    capture: Arc<MailCaptureService>,
}

async fn harness() -> Harness {
    let opts: sqlx::sqlite::SqliteConnectOptions = "sqlite::memory:".parse().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts.foreign_keys(true))
        .await
        .unwrap();
    for sql in [
        IDENTITY_V001,
        SITES_V001,
        SITE_STAGING_V001,
        SITE_STAGING_V002,
        MAIL_CAPTURE_V001,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let owner = owner();
    sqlx::query("INSERT INTO users (id, username, email, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(owner.id().to_string())
        .bind(owner.username().as_str())
        .bind(owner.email().as_str())
        .bind("not-a-real-hash")
        .bind("owner")
        .bind(Utc::now().to_rfc3339())
        .execute(&pool)
        .await
        .unwrap();
    let site = site(owner.id());
    let sites: Arc<dyn openpanel_domain::SiteRepository> =
        Arc::new(SqliteSiteRepository::new(pool.clone()));
    sites.insert(&site).await.unwrap();
    let staging = Arc::new(StagingService::new(
        Arc::new(SqliteStagingSlotRepository::new(pool.clone())),
        Arc::new(SqliteStagingSnapshotRepository::new(pool.clone())),
        Arc::new(SqlitePromotionRepository::new(pool.clone())),
        sites.clone(),
        Arc::new(InMemoryStagingFilesystem::new()),
        SqliteStagingLockTable::new(pool.clone()),
        Arc::new(MockAudit::stub()),
    ));
    let slot = staging
        .create_slot(
            &owner,
            site.id(),
            CreateSlotRequest {
                subdomain: None,
                document_root: None,
                sync_policy: None,
                php_version: None,
                schedule: None,
            },
        )
        .await
        .unwrap();
    assert!(!slot.mail_capture(), "capture must default off");
    let capture = Arc::new(MailCaptureService::new(
        pool.clone(),
        sites,
        [7u8; 32],
        Arc::new(MockAudit::stub()),
    ));
    Harness {
        pool,
        owner,
        site,
        slot_id: slot.id(),
        staging,
        capture,
    }
}

async fn enable(h: &Harness) {
    h.staging
        .set_mail_capture(&h.owner, h.site.id(), true)
        .await
        .unwrap();
}

#[tokio::test]
async fn capture_is_skipped_while_disabled_and_for_unknown_domains() {
    let h = harness().await;
    let skipped = h
        .capture
        .capture_for_mailbox(
            &format!("form@{DOMAIN}"),
            "visitor@elsewhere.test",
            "Contact form",
            PLAINTEXT,
        )
        .await
        .unwrap();
    assert!(!skipped, "toggle defaults off: no capture");
    let unknown = h
        .capture
        .capture_for_mailbox("form@nowhere.test", "v@elsewhere.test", "S", "b")
        .await
        .unwrap();
    assert!(!unknown, "unmapped domain passes through");
}

#[tokio::test]
async fn captured_body_is_encrypted_at_rest_and_readable_via_service() {
    let h = harness().await;
    enable(&h).await;
    let captured = h
        .capture
        .capture_for_mailbox(
            &format!("form@{DOMAIN}"),
            "visitor@elsewhere.test",
            "Contact form",
            PLAINTEXT,
        )
        .await
        .unwrap();
    assert!(captured);
    let listed = h.capture.list(&h.owner, h.site.id()).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].subject, "Contact form");
    // The list view is body-free.
    assert!(!serde_json::to_string(&listed).unwrap().contains(PLAINTEXT));
    let detail = h
        .capture
        .read(&h.owner, h.site.id(), listed[0].id)
        .await
        .unwrap();
    assert_eq!(detail.body, PLAINTEXT);
    // Raw storage never carries the plaintext body.
    let raw: String = sqlx::query_scalar("SELECT encrypted_body FROM mail_capture_messages")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert!(!raw.contains(PLAINTEXT), "body must be encrypted at rest");
    assert!(!raw.is_empty());
}

#[tokio::test]
async fn purge_empties_the_inbox() {
    let h = harness().await;
    enable(&h).await;
    for subject in ["first", "second"] {
        h.capture
            .capture_for_mailbox(&format!("form@{DOMAIN}"), "v@e.test", subject, "body")
            .await
            .unwrap();
    }
    let removed = h.capture.purge(&h.owner, h.site.id()).await.unwrap();
    assert_eq!(removed, 2);
    assert!(
        h.capture
            .list(&h.owner, h.site.id())
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn fifo_cap_evicts_oldest_messages_first() {
    let h = harness().await;
    let repo = SqliteMailCaptureRepository::new(h.pool.clone());
    let base = Utc::now();
    for n in 0..5i64 {
        let message = CapturedMessage::new(
            Uuid::new_v4(),
            h.site.id(),
            h.slot_id,
            format!("form@{DOMAIN}"),
            "v@e.test",
            format!("msg-{n}"),
            "body",
            base + Duration::seconds(n),
        )
        .unwrap();
        repo.insert(&message, "00:00", 3).await.unwrap();
    }
    let listed = repo.list_for_site(h.site.id()).await.unwrap();
    let subjects: Vec<&str> = listed.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(
        subjects,
        ["msg-4", "msg-3", "msg-2"],
        "newest kept, oldest evicted"
    );
}

#[tokio::test]
async fn destroy_slot_purges_inbox_via_fk_cascade() {
    let h = harness().await;
    enable(&h).await;
    h.capture
        .capture_for_mailbox(&format!("form@{DOMAIN}"), "v@e.test", "hi", PLAINTEXT)
        .await
        .unwrap();
    h.staging
        .delete_slot(&h.owner, h.site.id(), false)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM mail_capture_messages")
        .fetch_one(&h.pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "slot destroy must purge the inbox");
}

#[tokio::test]
async fn non_owner_cannot_read_the_inbox() {
    let h = harness().await;
    enable(&h).await;
    let stranger = User::new(
        Uuid::new_v4(),
        Username::new("intruder").unwrap(),
        Email::new("intruder@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::User,
    );
    let denied = h.capture.list(&stranger, h.site.id()).await;
    assert_eq!(denied.unwrap_err(), MailCaptureError::Forbidden);
}

#[tokio::test]
async fn webmail_diverts_when_capturing_and_passes_through_otherwise() {
    let h = harness().await;
    let bridge = Arc::new(InMemoryMailBridge::default());
    let webmail = Arc::new(openpanel_app::WebmailService::new(
        Arc::new(SqliteWebmailRepository::new(h.pool.clone())),
        bridge.clone(),
        Arc::new(MockAudit::stub()),
        [0u8; 32],
    ));
    webmail.attach_capture(h.capture.clone());
    let session = WebmailSessionToken::new(
        Uuid::new_v4(),
        format!("form@{DOMAIN}"),
        "tok",
        "cipher",
        "orig",
        Utc::now(),
    )
    .unwrap();
    let quota = MailboxQuota::new(100, 1, 1_000_000).unwrap();
    let policy = DomainSendingPolicy::default_for(DOMAIN);

    // Capture off: normal delivery through the bridge.
    let sent = webmail
        .send(
            &session,
            "v@e.test",
            "Hello",
            "<p>hi</p>",
            "hi",
            quota,
            0,
            policy.clone(),
        )
        .await
        .unwrap();
    assert!(!sent.captured);
    let delivered = bridge
        .list_messages(session.mailbox(), "INBOX", 10)
        .await
        .unwrap();
    assert_eq!(delivered.len(), 1);

    // Capture on: diverted, bridge is not called.
    enable(&h).await;
    let diverted = webmail
        .send(
            &session, "v@e.test", "Form", "<p>x</p>", "x", quota, 0, policy,
        )
        .await
        .unwrap();
    assert!(diverted.captured);
    let after = bridge
        .list_messages(session.mailbox(), "INBOX", 10)
        .await
        .unwrap();
    assert_eq!(after.len(), 1, "diverted mail must not reach the bridge");
    let inbox = h.capture.list(&h.owner, h.site.id()).await.unwrap();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].subject, "Form");
}

struct FailingPort;

#[async_trait]
impl MailCapturePort for FailingPort {
    async fn capture_for_mailbox(
        &self,
        _mailbox: &str,
        _recipient: &str,
        _subject: &str,
        _body: &str,
    ) -> Result<bool, MailCaptureError> {
        Err(MailCaptureError::Internal("store exploded".into()))
    }
    async fn enabled_for_site(&self, _site_id: Uuid) -> Result<bool, MailCaptureError> {
        Ok(true)
    }
    async fn purge_slot(&self, _slot_id: Uuid) -> Result<(), MailCaptureError> {
        Ok(())
    }
}

#[tokio::test]
async fn webmail_send_fails_closed_when_capture_storage_fails() {
    let h = harness().await;
    let bridge = Arc::new(InMemoryMailBridge::default());
    let webmail = openpanel_app::WebmailService::new(
        Arc::new(SqliteWebmailRepository::new(h.pool.clone())),
        bridge.clone(),
        Arc::new(MockAudit::stub()),
        [0u8; 32],
    );
    webmail.attach_capture(Arc::new(FailingPort));
    let session = WebmailSessionToken::new(
        Uuid::new_v4(),
        format!("form@{DOMAIN}"),
        "tok",
        "cipher",
        "orig",
        Utc::now(),
    )
    .unwrap();
    let quota = MailboxQuota::new(100, 1, 1_000_000).unwrap();
    let result = webmail
        .send(
            &session,
            "v@e.test",
            "Subject",
            "<p>b</p>",
            "b",
            quota,
            0,
            DomainSendingPolicy::default_for(DOMAIN),
        )
        .await;
    assert!(
        matches!(
            result.unwrap_err(),
            openpanel_domain::WebmailError::Bridge(_)
        ),
        "capture failure must refuse the send, never fall through to SMTP"
    );
}
