//! Load balancing and failover bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{LbError, LbStatus, Member, Pool, PoolAlgorithm, Role};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::load_balancing::{LbService, MemberRotator, SqliteLbRepository};

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

fn make_pool() -> Pool {
    Pool {
        id: Uuid::new_v4(),
        name: "primary".into(),
        algorithm: PoolAlgorithm::Weighted,
        created_at: Utc::now(),
    }
}

fn make_member(pool_id: Uuid, address: &str, weight: u32, status: LbStatus) -> Member {
    Member {
        id: Uuid::new_v4(),
        pool_id,
        address: address.into(),
        weight,
        status,
        last_probe_at: None,
        failed_probe_count: 0,
    }
}

#[tokio::test]
async fn create_pool_and_add_members() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create pool");
    service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.1:80", 10, LbStatus::Healthy),
        )
        .await
        .expect("add a");
    service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.2:80", 20, LbStatus::Healthy),
        )
        .await
        .expect("add b");
    let members = service.list_members(pool.id).await.expect("list");
    assert_eq!(members.len(), 2);
}

#[tokio::test]
async fn next_member_skips_disabled_and_failing() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let failing = service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.1:80", 10, LbStatus::Failing),
        )
        .await
        .expect("failing");
    let healthy = service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.2:80", 5, LbStatus::Healthy),
        )
        .await
        .expect("healthy");
    let decision = service.next(pool.id).await.expect("next");
    let member = decision.member.expect("present");
    assert_eq!(member.id, healthy.id);
    let _ = failing;
}

#[tokio::test]
async fn apply_probe_demotes_then_recovers() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let m = service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.1:80", 10, LbStatus::Healthy),
        )
        .await
        .expect("add");
    for _ in 0..3 {
        let _ = service
            .apply_probe(&admin_user(), m.id, false, 3, 1)
            .await
            .expect("probe fail");
    }
    let members = service.list_members(pool.id).await.expect("list");
    let m_after = members.iter().find(|x| x.id == m.id).expect("present");
    assert_eq!(m_after.status, LbStatus::Failing);
    let _ = service
        .apply_probe(&admin_user(), m.id, true, 3, 1)
        .await
        .expect("probe ok");
    let members = service.list_members(pool.id).await.expect("list");
    let m_after = members.iter().find(|x| x.id == m.id).expect("present");
    assert_eq!(m_after.status, LbStatus::Healthy);
}

#[tokio::test]
async fn member_validation_rejects_invalid_weight() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let mut m = make_member(pool.id, "10.0.0.1:80", 200, LbStatus::Healthy);
    let res = service.add_member(&admin_user(), m.clone()).await;
    assert!(matches!(res, Err(LbError::InvalidWeight(_))));
    m.weight = 10;
    m.address = "no-port".into();
    let res = service.add_member(&admin_user(), m).await;
    assert!(matches!(res, Err(LbError::InvalidWeight(_))));
}

#[tokio::test]
async fn next_member_returns_none_for_empty_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let decision = service.next(pool.id).await.expect("next");
    assert!(decision.member.is_none());
}

#[tokio::test]
async fn apply_probe_records_audit_metadata() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let audit = Arc::new(NoopAuditService);
    let rotator = MemberRotator::new(repo.clone(), audit.clone());
    let service = LbService::new(repo.clone(), rotator);
    let pool = make_pool();
    service
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let m = service
        .add_member(
            &admin_user(),
            make_member(pool.id, "10.0.0.1:80", 10, LbStatus::Healthy),
        )
        .await
        .expect("add");
    let decision = service
        .apply_probe(&admin_user(), m.id, true, 3, 1)
        .await
        .expect("probe");
    assert!(decision.ok);
}

#[tokio::test]
async fn non_admin_cannot_create_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteLbRepository::new(db.pool()));
    let rotator = MemberRotator::new(repo.clone(), Arc::new(NoopAuditService));
    let service = LbService::new(repo.clone(), rotator);
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = service.create_pool(&user, make_pool()).await;
    assert!(matches!(res, Err(LbError::Forbidden)));
}
