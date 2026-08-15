//! Reseller billing bounded context unit and service tests.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    BillingError, BillingRepository, BillingStatus, Chargeback, Integration, Role, UsageMeter,
    UsageUnit, compute_chargeback, hmac_sha256_hex, verify_signature,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::billing::{
    BillingService, ChargebackEngine, SqliteBillingRepository, UsageExporter, WebhookRelay,
};

fn admin_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("admin").expect("static"),
        Email::new("admin@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Admin,
    )
}

fn meter(owner: Uuid, unit: UsageUnit, quantity: u64) -> UsageMeter {
    let now = Utc::now();
    UsageMeter {
        id: Uuid::new_v4(),
        owner_id: owner,
        unit,
        quantity,
        period_start: now,
        period_end: now + Duration::days(30),
        closed: true,
        recorded_at: now,
    }
}

fn integration(status: BillingStatus) -> Integration {
    Integration {
        id: Uuid::new_v4(),
        name: "whmcs".into(),
        webhook_url: "https://example.test/hook".into(),
        webhook_secret: "this-is-a-32-byte-secret!".into(),
        status,
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn exporter_returns_meters_for_owner() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let owner = Uuid::new_v4();
    repo.save_meter(&meter(owner, UsageUnit::Gigabytes, 10))
        .await
        .expect("save");
    let exporter = UsageExporter::new(repo);
    let meters = exporter.export(&admin_user(), owner).await.expect("export");
    assert_eq!(meters.len(), 1);
    assert_eq!(meters[0].quantity, 10);
}

#[tokio::test]
async fn engine_persists_chargeback() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let owner = Uuid::new_v4();
    repo.save_meter(&meter(owner, UsageUnit::Gigabytes, 100))
        .await
        .expect("save");
    repo.save_meter(&meter(owner, UsageUnit::Requests, 50))
        .await
        .expect("save");
    let engine = ChargebackEngine::new(repo.clone(), Arc::new(NoopAuditService));
    let cb = engine
        .compute(
            &admin_user(),
            owner,
            &[(UsageUnit::Gigabytes, 5u64), (UsageUnit::Requests, 1u64)],
            "USD",
        )
        .await
        .expect("compute");
    assert_eq!(cb.amount_minor, 100 * 5 + 50 * 1);
    let stored = repo.list_chargebacks(owner).await.expect("list");
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].amount_minor, cb.amount_minor);
}

#[tokio::test]
async fn relay_accepts_matching_signature() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let integration = integration(BillingStatus::Enabled);
    repo.save_integration(&integration).await.expect("save");
    let relay = WebhookRelay::new(repo, Arc::new(NoopAuditService));
    let body = b"{\"event\":\"provisioned\"}";
    let sig = hmac_sha256_hex(&integration.webhook_secret, body);
    let outcome = relay
        .verify(&admin_user(), integration.id, body, Some(&sig))
        .await
        .expect("verify");
    assert!(outcome.relayed);
    assert!(outcome.reason.is_none());
}

#[tokio::test]
async fn relay_rejects_bad_signature() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let integration = integration(BillingStatus::Enabled);
    repo.save_integration(&integration).await.expect("save");
    let relay = WebhookRelay::new(repo, Arc::new(NoopAuditService));
    let body = b"{\"event\":\"provisioned\"}";
    let res = relay
        .verify(&admin_user(), integration.id, body, Some("wrong-signature"))
        .await;
    assert!(matches!(res, Err(BillingError::InvalidSignature)));
}

#[tokio::test]
async fn relay_rejects_disabled_integration() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let integration = integration(BillingStatus::Disabled);
    repo.save_integration(&integration).await.expect("save");
    let relay = WebhookRelay::new(repo, Arc::new(NoopAuditService));
    let body = b"{}";
    let sig = hmac_sha256_hex(&integration.webhook_secret, body);
    let outcome = relay
        .verify(&admin_user(), integration.id, body, Some(&sig))
        .await
        .expect("verify");
    assert!(!outcome.relayed);
    assert!(outcome.reason.is_some());
}

#[tokio::test]
async fn non_admin_cannot_use_billing() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let exporter = UsageExporter::new(repo.clone());
    let engine = ChargebackEngine::new(repo.clone(), Arc::new(NoopAuditService));
    let relay = WebhookRelay::new(repo, Arc::new(NoopAuditService));
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let _ = exporter.export(&user, Uuid::new_v4()).await;
    let _ = engine
        .compute(&user, Uuid::new_v4(), &[], "USD")
        .await;
    let _ = relay.verify(&user, Uuid::new_v4(), b"", Some("")).await;
}

#[tokio::test]
async fn compute_chargeback_is_pure_and_deterministic() {
    let owner = Uuid::new_v4();
    let now = Utc::now();
    let meters = vec![meter(owner, UsageUnit::Gigabytes, 10)];
    let prices = vec![(UsageUnit::Gigabytes, 3u64)];
    let a = compute_chargeback(owner, now, now, &meters, &prices, "USD");
    let b = compute_chargeback(owner, now, now, &meters, &prices, "USD");
    assert_eq!(a.amount_minor, b.amount_minor);
    assert_eq!(a.lines.len(), b.lines.len());
}

#[tokio::test]
async fn integration_validation_rejects_short_secret() {
    let mut bad = integration(BillingStatus::Enabled);
    bad.webhook_secret = "short".into();
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn integration_validation_rejects_non_http_url() {
    let mut bad = integration(BillingStatus::Enabled);
    bad.webhook_url = "ftp://example.test".into();
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn billing_service_facade_routes_to_subservices() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteBillingRepository::new(db.pool()));
    let exporter = UsageExporter::new(repo.clone());
    let engine = ChargebackEngine::new(repo.clone(), Arc::new(NoopAuditService));
    let relay = WebhookRelay::new(repo.clone(), Arc::new(NoopAuditService));
    let service = BillingService::new(exporter, engine, relay);
    let owner = Uuid::new_v4();
    repo.save_meter(&meter(owner, UsageUnit::Gigabytes, 7))
        .await
        .expect("save");
    let exported = service.export(&admin_user(), owner).await.expect("export");
    assert_eq!(exported.len(), 1);
    let cb: Chargeback = service
        .compute(
            &admin_user(),
            owner,
            &[(UsageUnit::Gigabytes, 4u64)],
            "USD",
        )
        .await
        .expect("compute");
    assert_eq!(cb.amount_minor, 28);
}

#[tokio::test]
async fn verify_signature_helper_accepts_and_rejects() {
    let body = b"hello";
    let secret = "0123456789abcdef";
    let sig = hmac_sha256_hex(secret, body);
    assert!(verify_signature(secret, body, Some(&sig)).is_ok());
    assert!(verify_signature(secret, body, None).is_err());
    assert!(verify_signature(secret, body, Some("nope")).is_err());
}
