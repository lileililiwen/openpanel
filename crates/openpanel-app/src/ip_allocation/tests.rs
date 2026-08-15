//! IPv6 + address-pool bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{IpError, IpFamily, IpRepository, IpStatus, PoolKind, Role};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::ip_allocation::{Allocator, SqliteIpRepository, VhostBinder};

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

fn make_v4_pool() -> openpanel_domain::IpPool {
    openpanel_domain::IpPool {
        id: Uuid::new_v4(),
        name: "primary-v4".into(),
        kind: PoolKind::Shared,
        family: IpFamily::V4,
        cidr: "10.0.0.0/24".into(),
        created_at: Utc::now(),
    }
}

fn make_v6_pool() -> openpanel_domain::IpPool {
    openpanel_domain::IpPool {
        id: Uuid::new_v4(),
        name: "primary-v6".into(),
        kind: PoolKind::Dedicated,
        family: IpFamily::V6,
        cidr: "2001:db8::/32".into(),
        created_at: Utc::now(),
    }
}

#[tokio::test]
async fn create_pool_persists_and_validates() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let pool = make_v4_pool();
    let saved = allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let listed = repo.list_pools().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, saved.id);
}

#[tokio::test]
async fn create_pool_rejects_malformed_cidr() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo, Arc::new(NoopAuditService));
    let mut pool = make_v4_pool();
    pool.cidr = "not-a-cidr".into();
    let res = allocator.create_pool(&admin_user(), pool).await;
    assert!(matches!(res, Err(IpError::InvalidCidr(_))));
}

#[tokio::test]
async fn allocate_picks_first_free_address() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let pool = make_v4_pool();
    allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let a1 = allocator
        .allocate(&admin_user(), pool.id, Uuid::new_v4())
        .await
        .expect("a1");
    let a2 = allocator
        .allocate(&admin_user(), pool.id, Uuid::new_v4())
        .await
        .expect("a2");
    assert_ne!(a1.address, a2.address);
}

#[tokio::test]
async fn reserve_specific_address_in_v6_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let pool = make_v6_pool();
    allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let a1 = allocator
        .reserve(&admin_user(), pool.id, Uuid::new_v4(), "2001:db8::1")
        .await
        .expect("reserve");
    assert_eq!(a1.address, "2001:db8::1");
    let res = allocator
        .reserve(&admin_user(), pool.id, Uuid::new_v4(), "2001:db8::1")
        .await;
    assert!(matches!(res, Err(IpError::AddressInUse(_))));
}

#[tokio::test]
async fn reserve_rejects_address_outside_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo, Arc::new(NoopAuditService));
    let pool = make_v4_pool();
    allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let res = allocator
        .reserve(&admin_user(), pool.id, Uuid::new_v4(), "99.99.99.99")
        .await;
    assert!(matches!(res, Err(IpError::OutsidePool(_))));
}

#[tokio::test]
async fn dedicated_pool_never_assigns_to_two_sites() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let mut pool = make_v6_pool();
    pool.kind = PoolKind::Dedicated;
    allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let site_a = Uuid::new_v4();
    let site_b = Uuid::new_v4();
    let a1 = allocator
        .reserve(&admin_user(), pool.id, site_a, "2001:db8::1")
        .await
        .expect("a1");
    assert_eq!(a1.site_id, site_a);
    let res = allocator
        .reserve(&admin_user(), pool.id, site_b, "2001:db8::1")
        .await;
    assert!(matches!(res, Err(IpError::AddressInUse(_))));
}

#[tokio::test]
async fn release_returns_address_to_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let binder = VhostBinder::new(repo.clone(), Arc::new(NoopAuditService));
    let pool = make_v4_pool();
    allocator
        .create_pool(&admin_user(), pool.clone())
        .await
        .expect("create");
    let a1 = allocator
        .reserve(&admin_user(), pool.id, Uuid::new_v4(), "10.0.0.99")
        .await
        .expect("reserve");
    let _ = binder
        .release(&admin_user(), pool.id, &a1.address)
        .await
        .expect("release");
    let after = repo
        .get_allocation_by_address(pool.id, &a1.address)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(after.status, IpStatus::Released);
}

#[tokio::test]
async fn rebind_groups_addresses_by_family() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo.clone(), Arc::new(NoopAuditService));
    let binder = VhostBinder::new(repo, Arc::new(NoopAuditService));
    let v4 = make_v4_pool();
    let mut v6 = make_v6_pool();
    v6.kind = PoolKind::Shared;
    allocator
        .create_pool(&admin_user(), v4.clone())
        .await
        .expect("create v4");
    allocator
        .create_pool(&admin_user(), v6.clone())
        .await
        .expect("create v6");
    let site_id = Uuid::new_v4();
    let _ = allocator
        .reserve(&admin_user(), v4.id, site_id, "10.0.0.10")
        .await
        .expect("reserve v4");
    let _ = allocator
        .reserve(&admin_user(), v6.id, site_id, "2001:db8::42")
        .await
        .expect("reserve v6");
    let address = binder.rebind(&admin_user(), site_id).await.expect("rebind");
    assert_eq!(address.v4, vec!["10.0.0.10".to_string()]);
    assert_eq!(address.v6, vec!["2001:db8::42".to_string()]);
}

#[tokio::test]
async fn non_admin_cannot_create_pool() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIpRepository::new(db.pool()));
    let allocator = Allocator::new(repo, Arc::new(NoopAuditService));
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = allocator.create_pool(&user, make_v4_pool()).await;
    assert!(matches!(res, Err(IpError::Forbidden)));
}

#[tokio::test]
async fn candidate_addresses_bounded_by_limit() {
    use crate::ip_allocation::candidate_addresses;
    let pool = make_v4_pool();
    let list = candidate_addresses(&pool, 3);
    assert_eq!(list.len(), 3);
    let pool = make_v6_pool();
    let list = candidate_addresses(&pool, 5);
    assert_eq!(list.len(), 5);
}
