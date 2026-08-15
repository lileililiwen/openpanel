//! DNSSEC + secondary DNS bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    DsRecord, DnsSecError, DnsSecPolicy, DnsSecRepository, GlueRecord, KeyRole, Role,
    SecondaryNs, SigningAlgorithm,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::dnssec_secondary::{
    AxfrSender, DnsSecService, GlueRecordService, KeyRolloverEngine, RecordingRegistrar,
    SqliteDnsSecRepository,
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

fn make_secondary(allowed: Vec<String>) -> SecondaryNs {
    SecondaryNs {
        id: Uuid::new_v4(),
        zone_id: Uuid::new_v4(),
        address: "ns1.secondary.test".into(),
        allowed_cidrs: allowed,
        added_at: Utc::now(),
    }
}

#[tokio::test]
async fn enable_dnssec_persists_policy() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let registrar = Arc::new(RecordingRegistrar::new());
    let service = DnsSecService::new(repo.clone(), Arc::new(NoopAuditService), registrar);
    let zone_id = Uuid::new_v4();
    let policy = service
        .enable(&admin_user(), zone_id, SigningAlgorithm::Ecdsap256sha256)
        .await
        .expect("enable");
    assert!(policy.enabled);
    let loaded = repo.get_policy(zone_id).await.expect("get").expect("present");
    assert!(loaded.enabled);
}

#[tokio::test]
async fn disable_dnssec_requires_existing_policy() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let registrar = Arc::new(RecordingRegistrar::new());
    let service = DnsSecService::new(repo, Arc::new(NoopAuditService), registrar);
    let res = service.disable(&admin_user(), Uuid::new_v4()).await;
    assert!(matches!(res, Err(DnsSecError::NotSigned(_))));
}

#[tokio::test]
async fn add_key_persists_public_metadata_only() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let registrar = Arc::new(RecordingRegistrar::new());
    let service = DnsSecService::new(repo.clone(), Arc::new(NoopAuditService), registrar);
    let zone_id = Uuid::new_v4();
    let key = openpanel_domain::ZoneSigningKey {
        id: Uuid::new_v4(),
        zone_id,
        role: KeyRole::Ksk,
        algorithm: SigningAlgorithm::Ecdsap256sha256,
        key_tag: 12345,
        public_digest: "abcd".into(),
        active: true,
        rollover_in_progress: false,
        created_at: Utc::now(),
    };
    service.add_key(&admin_user(), key.clone()).await.expect("add");
    let listed = service.list_keys(zone_id).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].key_tag, 12345);
}

#[tokio::test]
async fn publish_ds_persists_and_calls_registrar() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let registrar = Arc::new(RecordingRegistrar::new());
    let service = DnsSecService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        registrar.clone(),
    );
    let zone_id = Uuid::new_v4();
    let ds = DsRecord {
        zone_id,
        key_tag: 12345,
        algorithm: 13,
        digest_type: 2,
        digest: "a".repeat(64),
    };
    service
        .publish_ds(&admin_user(), zone_id, ds.clone())
        .await
        .expect("publish");
    let listed = repo.list_ds(zone_id).await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(registrar.publishes().len(), 1);
}

#[tokio::test]
async fn publish_ds_rejects_bad_digest() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let registrar = Arc::new(RecordingRegistrar::new());
    let service = DnsSecService::new(repo, Arc::new(NoopAuditService), registrar);
    let zone_id = Uuid::new_v4();
    let ds = DsRecord {
        zone_id,
        key_tag: 12345,
        algorithm: 13,
        digest_type: 2,
        digest: "abc".into(),
    };
    let res = service.publish_ds(&admin_user(), zone_id, ds).await;
    assert!(matches!(res, Err(DnsSecError::InvalidDigest(_))));
}

#[tokio::test]
async fn axfr_sender_respects_acl() {
    let sender = AxfrSender::new();
    let a = make_secondary(vec!["10.0.0.1".into()]);
    let b = make_secondary(vec!["10.0.0.2".into()]);
    let secondaries = vec![a, b];
    let allowed = sender.ship(&secondaries, "10.0.0.1").expect("ship");
    assert_eq!(allowed.len(), 1);
    let res = sender.ship(&secondaries, "10.0.0.99");
    assert!(matches!(res, Err(DnsSecError::SecondaryRejected(_))));
}

#[tokio::test]
async fn glue_service_rejects_empty_addresses() {
    let svc = GlueRecordService::new();
    let bad = GlueRecord {
        id: Uuid::new_v4(),
        zone_id: Uuid::new_v4(),
        name: "ns1.example.com".into(),
        a: None,
        aaaa: None,
    };
    assert!(svc.validate(&bad).is_err());
    let good = GlueRecord {
        a: Some("10.0.0.1".into()),
        ..bad
    };
    assert!(svc.validate(&good).is_ok());
}

#[test]
fn ksk_state_machine_cycles_through_three_states() {
    use openpanel_domain::KskRolloverState::*;
    let engine = KeyRolloverEngine::new();
    assert_eq!(engine.advance(SingleActive), DoubleSign);
    assert_eq!(engine.advance(DoubleSign), NewActive);
    assert_eq!(engine.advance(NewActive), SingleActive);
}

#[tokio::test]
async fn secondary_acl_allows_exact_match() {
    let s = make_secondary(vec!["ns1.secondary.test".into()]);
    assert!(s.allows("ns1.secondary.test"));
    assert!(!s.allows("ns2.secondary.test"));
}

#[tokio::test]
async fn policy_round_trip_through_repo() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDnsSecRepository::new(db.pool()));
    let zone_id = Uuid::new_v4();
    let policy = DnsSecPolicy {
        zone_id,
        enabled: true,
        algorithm: SigningAlgorithm::Ed25519,
        enabled_at: Some(Utc::now()),
    };
    repo.save_policy(&policy).await.expect("save");
    let loaded = repo.get_policy(zone_id).await.expect("get").expect("present");
    assert!(loaded.enabled);
    assert_eq!(loaded.algorithm, SigningAlgorithm::Ed25519);
}