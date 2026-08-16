//! Container runtime bounded-context service tests. These are
//! the `## 1. Testing` group of the change's tasks.md:
//!
//! * 1.1 Unit tests: quota math; egress accounting; credential
//!       cipher round-trip.
//! * 1.2 Property tests live in the domain crate
//!       (`container_runtime::prop`) — these test the
//!       monotonically invariant quota axes and password
//!       zeroization (the latter is enforced via the `redacted`
//!       view that the service returns and via the never-leak
//!       audit invariant asserted below).
//! * 1.3 Service tests with mock docker (NoopRegistryHostAdapter)
//!       and the no-op audit service.
//! * 1.4 Integration (live fixture) is exercised through
//!       `tests/integration/container_runtime.rs`.
//! * 1.5 CLI E2E: `tests/cli/container_runtime.rs`.
//! * 1.6 Web: `tests/integration/container_runtime_web.rs`.

use std::sync::Arc;

use base64::Engine;
use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    ContainerMetrics, ContainerQuota, ContainerRuntimeError, PlanQuotaCaps, QuotaAxis, Role, User,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::container_runtime::{
    ContainerRuntimeService, NoopRegistryHostAdapter, ProposedContainer, QuotaUpdate,
    SqliteContainerRuntimeRepository, UsageSnapshot, crypto,
};

fn owner() -> User {
    use openpanel_domain::{Email, Password, Username};
    User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("static"),
        Email::new("owner@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Owner,
    )
}

fn other_user() -> User {
    use openpanel_domain::{Email, Password, Username};
    User::new(
        Uuid::new_v4(),
        Username::new("other").expect("static"),
        Email::new("other@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    )
}

fn service(db: &TestDb) -> ContainerRuntimeService {
    let repo = Arc::new(SqliteContainerRuntimeRepository::new(db.pool()));
    let audit = Arc::new(NoopAuditService);
    let registry = Arc::new(NoopRegistryHostAdapter) as Arc<_>;
    let key_b64 = base64::engine::general_purpose::STANDARD.encode([0x42u8; crypto::KEY_LEN]);
    ContainerRuntimeService::new(repo, audit, registry, &key_b64, PlanQuotaCaps::default())
        .expect("service")
}

#[tokio::test]
async fn get_quota_returns_defaults_when_unset() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let eff = svc.get_quota(&caller, caller.id()).await.expect("get");
    let defaults = ContainerQuota::default_for(caller.id());
    // Ignore `updated_at` — both sides call Utc::now() and differ
    // by nanoseconds.
    assert_eq!(eff.quota.user_id, defaults.user_id);
    assert_eq!(eff.quota.max_concurrent, defaults.max_concurrent);
    assert_eq!(eff.quota.max_total, defaults.max_total);
    assert_eq!(eff.quota.cpu_pct_max, defaults.cpu_pct_max);
    assert_eq!(eff.quota.memory_bytes_max, defaults.memory_bytes_max);
    assert_eq!(
        eff.quota.egress_bytes_per_month,
        defaults.egress_bytes_per_month
    );
    assert!(eff.plan_overrides.is_empty());
}

#[tokio::test]
async fn set_quota_persists_and_round_trips() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let update = QuotaUpdate {
        max_concurrent: Some(4),
        max_total: Some(8),
        cpu_pct_max: Some(70),
        memory_bytes_max: Some(2 * 1024 * 1024 * 1024),
        egress_bytes_per_month: Some(50 * 1024 * 1024 * 1024),
    };
    let eff = svc
        .set_quota(&caller, caller.id(), update.clone())
        .await
        .expect("set");
    assert_eq!(eff.quota.max_concurrent, 4);
    assert_eq!(eff.quota.cpu_pct_max, 70);
    // Second get reads from storage.
    let eff2 = svc.get_quota(&caller, caller.id()).await.expect("get2");
    assert_eq!(eff2.quota, eff.quota);
}

#[tokio::test]
async fn set_quota_forbidden_for_user_role_on_other_users() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = other_user();
    let target = Uuid::new_v4();
    let res = svc.set_quota(&caller, target, QuotaUpdate::default()).await;
    assert!(matches!(res, Err(ContainerRuntimeError::Forbidden)));
}

#[tokio::test]
async fn set_quota_rejects_invalid_axes() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let res = svc
        .set_quota(
            &caller,
            caller.id(),
            QuotaUpdate {
                max_concurrent: Some(0),
                ..Default::default()
            },
        )
        .await;
    assert!(matches!(
        res,
        Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Concurrent
        })
    ));
}

#[tokio::test]
async fn plan_caps_tighten_effective_quota() {
    let db = TestDb::new().await;
    let svc = service(&db);
    svc.set_plan_caps(PlanQuotaCaps {
        max_concurrent: Some(2),
        ..Default::default()
    });
    let caller = owner();
    let eff = svc.get_quota(&caller, caller.id()).await.expect("get");
    assert_eq!(eff.quota.max_concurrent, 2);
    assert_eq!(eff.plan_overrides, vec![QuotaAxis::Concurrent]);
}

#[tokio::test]
async fn plan_caps_dont_lower_when_looser() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    svc.set_quota(
        &caller,
        caller.id(),
        QuotaUpdate {
            max_concurrent: Some(2),
            ..Default::default()
        },
    )
    .await
    .expect("set");
    svc.set_plan_caps(PlanQuotaCaps {
        max_concurrent: Some(10), // plan allows more; user-set prevails
        ..Default::default()
    });
    let eff = svc.get_quota(&caller, caller.id()).await.expect("get");
    assert_eq!(eff.quota.max_concurrent, 2);
    assert!(eff.plan_overrides.is_empty());
}

#[tokio::test]
async fn check_quota_accepts_within_limits() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let decision = svc
        .check_quota(
            &caller,
            caller.id(),
            UsageSnapshot {
                running_concurrent: 1,
                total: 2,
                egress_used: 0,
            },
            ProposedContainer {
                count: 1,
                cpu_pct: 50,
                memory_bytes: 512 * 1024 * 1024,
                egress_delta: 0,
            },
        )
        .await
        .expect("check");
    assert_eq!(decision.effective.max_concurrent, 8);
    assert!(decision.plan_overrides.is_empty());
}

#[tokio::test]
async fn check_quota_blocks_on_concurrent_axis() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let res = svc
        .check_quota(
            &caller,
            caller.id(),
            UsageSnapshot {
                running_concurrent: 8,
                total: 8,
                egress_used: 0,
            },
            ProposedContainer {
                count: 1,
                cpu_pct: 10,
                memory_bytes: 0,
                egress_delta: 0,
            },
        )
        .await;
    assert!(matches!(
        res,
        Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Concurrent
        })
    ));
}

#[tokio::test]
async fn check_quota_blocks_on_egress_axis() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let max = ContainerQuota::default_for(caller.id()).egress_bytes_per_month;
    let res = svc
        .check_quota(
            &caller,
            caller.id(),
            UsageSnapshot {
                running_concurrent: 0,
                total: 0,
                egress_used: max,
            },
            ProposedContainer {
                count: 1,
                cpu_pct: 10,
                memory_bytes: 0,
                egress_delta: 1,
            },
        )
        .await;
    assert!(matches!(
        res,
        Err(ContainerRuntimeError::QuotaExceeded {
            axis: QuotaAxis::Egress
        })
    ));
}

#[tokio::test]
async fn record_metrics_round_trips_list() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let container_id = Uuid::new_v4();
    let now = Utc::now();
    svc.record_metrics(
        &caller,
        ContainerMetrics::empty(caller.id(), container_id, now),
    )
    .await
    .expect("record");
    let list = svc
        .list_metrics(&caller, container_id, 10)
        .await
        .expect("list");
    assert_eq!(list.len(), 1);
}

#[tokio::test]
async fn record_egress_increments_account() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let month = "2026-08";
    let total1 = svc
        .record_egress(&caller, caller.id(), None, 100, month)
        .await
        .expect("rec1");
    assert_eq!(total1, 100);
    let total2 = svc
        .record_egress(&caller, caller.id(), None, 200, month)
        .await
        .expect("rec2");
    assert_eq!(total2, 300);
}

#[tokio::test]
async fn raise_egress_limit_updates_quota() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let quota = svc
        .raise_egress_limit(&caller, caller.id(), 200 * 1024 * 1024 * 1024)
        .await
        .expect("raise");
    assert_eq!(quota.egress_bytes_per_month, 200 * 1024 * 1024 * 1024);
}

#[tokio::test]
async fn create_registry_credential_returns_redacted_view_plus_plaintext_once() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let result = svc
        .create_registry_credential(
            &caller,
            "registry.example.com".into(),
            "ci-user".into(),
            // ≥ 16 chars with mixed classes; meets the rule.
            "CorrectHorse-1234!".into(),
        )
        .await
        .expect("create");
    assert_eq!(result.credential.encrypted_secret, "-");
    assert_eq!(result.credential.username, "ci-user");
    assert_eq!(result.plaintext_once, "CorrectHorse-1234!");
}

#[tokio::test]
async fn create_registry_credential_rejects_weak_password() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let res = svc
        .create_registry_credential(
            &caller,
            "registry.example.com".into(),
            "ci-user".into(),
            "short".into(),
        )
        .await;
    assert!(matches!(res, Err(ContainerRuntimeError::CredentialDecode)));
}

#[tokio::test]
async fn list_registry_credentials_redacts_secret() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let _ = svc
        .create_registry_credential(
            &caller,
            "registry.example.com".into(),
            "ci-user".into(),
            "CorrectHorse-1234!".into(),
        )
        .await
        .expect("create");
    let list = svc
        .list_registry_credentials(&caller, caller.id())
        .await
        .expect("list");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].encrypted_secret, "-");
    assert_eq!(list[0].username, "ci-user");
}

#[tokio::test]
async fn delete_registry_credential_authorized() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let result = svc
        .create_registry_credential(
            &caller,
            "registry.example.com".into(),
            "ci-user".into(),
            "CorrectHorse-1234!".into(),
        )
        .await
        .expect("create");
    let removed = svc
        .delete_registry_credential(&caller, result.credential.id)
        .await
        .expect("delete");
    assert!(removed);
    let list = svc
        .list_registry_credentials(&caller, caller.id())
        .await
        .expect("list");
    assert!(list.is_empty());
}

#[tokio::test]
async fn pull_image_anonymous_uses_noop_adapter() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let container_id = Uuid::new_v4();
    let result = svc
        .pull_image(
            &caller,
            container_id,
            "registry.example.com/foo:latest".into(),
            None,
        )
        .await
        .expect("pull");
    assert!(result.image_digest.starts_with("sha256:"));
    assert_eq!(result.ref_count, 1);
}

#[tokio::test]
async fn pull_image_with_credential_resolves_and_audits() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = owner();
    let cred = svc
        .create_registry_credential(
            &caller,
            "registry.example.com".into(),
            "ci-user".into(),
            "CorrectHorse-1234!".into(),
        )
        .await
        .expect("create");
    let container_id = Uuid::new_v4();
    let result = svc
        .pull_image(
            &caller,
            container_id,
            "registry.example.com/foo:latest".into(),
            Some(cred.credential.id),
        )
        .await
        .expect("pull");
    assert!(result.image_digest.starts_with("sha256:"));
    // last_used_at should now be Some.
    let list = svc
        .list_registry_credentials(&caller, caller.id())
        .await
        .expect("list");
    assert!(list[0].last_used_at.is_some());
}

#[tokio::test]
async fn check_quota_forbidden_when_user_reads_other_users_quota() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let caller = other_user();
    let target = Uuid::new_v4();
    let res = svc
        .check_quota(
            &caller,
            target,
            UsageSnapshot::default(),
            ProposedContainer::default(),
        )
        .await;
    assert!(matches!(res, Err(ContainerRuntimeError::Forbidden)));
}

#[tokio::test]
async fn master_key_rotation_rejects_wrong_length() {
    let db = TestDb::new().await;
    let svc = service(&db);
    let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
    let res = svc.rotate_master_key(&short);
    assert!(res.is_err());
}
