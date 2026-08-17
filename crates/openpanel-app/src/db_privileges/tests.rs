//! Database privilege management bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{GrantScope, Privilege, Role};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::db_privileges::{
    AdminToolSso, PrivilegeService, RemoteAccessController, SqliteDbPrivilegeRepository,
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

fn non_admin_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("viewer").expect("static"),
        Email::new("viewer@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    )
}

fn make_grant(database_id: Uuid, user_id: Uuid, scope: GrantScope) -> openpanel_domain::DbGrant {
    openpanel_domain::DbGrant {
        id: Uuid::new_v4(),
        database_id,
        user_id,
        scope,
        privilege: Privilege::Read,
        granted_by: Uuid::nil(),
        granted_at: Utc::now(),
    }
}

#[tokio::test]
async fn apply_grant_persists_and_audits() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let service = PrivilegeService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let database_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let grant = make_grant(database_id, user_id, GrantScope::Database);
    let applied = service.apply(&caller, grant.clone()).await.expect("apply");
    assert_eq!(applied.privilege, grant.privilege);
    let loaded = service
        .list(&caller, database_id)
        .await
        .expect("list")
        .pop()
        .expect("present");
    assert_eq!(loaded.id, applied.id);
}

#[tokio::test]
async fn revoke_grant_removes_row() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let service = PrivilegeService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let database_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let grant = make_grant(database_id, user_id, GrantScope::Database);
    let applied = service.apply(&caller, grant).await.expect("apply");
    service.revoke(&caller, applied.id).await.expect("revoke");
    let grants = service.list(&caller, database_id).await.expect("list");
    assert!(grants.is_empty());
}

#[tokio::test]
async fn non_admin_cannot_apply_grant() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let service = PrivilegeService::new(repo, Arc::new(NoopAuditService));
    let caller = non_admin_user();
    let res = service
        .apply(
            &caller,
            make_grant(Uuid::new_v4(), Uuid::new_v4(), GrantScope::Database),
        )
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DbPrivilegeError::Forbidden)
    ));
}

#[tokio::test]
async fn grant_rejects_table_with_sql_injection_attempt() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let service = PrivilegeService::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let bad = make_grant(
        Uuid::new_v4(),
        Uuid::new_v4(),
        GrantScope::Table {
            name: "users; DROP TABLE users;--".into(),
        },
    );
    let res = service.apply(&caller, bad).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DbPrivilegeError::OutsideDatabase(_))
    ));
}

#[tokio::test]
async fn remote_access_rejects_empty_acl_when_enabling() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let controller = RemoteAccessController::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let res = controller
        .set(&caller, Uuid::new_v4(), true, Vec::new(), false)
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DbPrivilegeError::EmptyAcl)
    ));
}

#[tokio::test]
async fn remote_access_rejects_wildcard_without_opt_in() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let controller = RemoteAccessController::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let res = controller
        .set(
            &caller,
            Uuid::new_v4(),
            true,
            vec!["0.0.0.0/0".into()],
            false,
        )
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DbPrivilegeError::WildcardAcl)
    ));
}

#[tokio::test]
async fn remote_access_accepts_wildcard_with_opt_in() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let controller = RemoteAccessController::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let database_id = Uuid::new_v4();
    let access = controller
        .set(&caller, database_id, true, vec!["0.0.0.0/0".into()], true)
        .await
        .expect("set");
    assert!(access.wildcard_opt_in);
    let loaded = controller.get(&caller, database_id).await.expect("get");
    assert!(loaded.enabled);
    assert_eq!(loaded.allow_cidrs, vec!["0.0.0.0/0".to_string()]);
}

#[tokio::test]
async fn sso_session_issued_and_consumed_once() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let sso = AdminToolSso::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let database_id = Uuid::new_v4();
    let user_id = Uuid::new_v4();
    let session = sso
        .issue(&caller, database_id, user_id)
        .await
        .expect("issue");
    let consumed = sso
        .consume(&caller, session.id, &session.token)
        .await
        .expect("consume");
    assert!(consumed.consumed_at.is_some());
    let second = sso.consume(&caller, session.id, &session.token).await;
    assert!(matches!(
        second,
        Err(openpanel_domain::DbPrivilegeError::InvalidSsoToken)
    ));
}

#[tokio::test]
async fn sso_session_rejects_wrong_token() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let sso = AdminToolSso::new(repo, Arc::new(NoopAuditService));
    let caller = admin_user();
    let session = sso
        .issue(&caller, Uuid::new_v4(), Uuid::new_v4())
        .await
        .expect("issue");
    let res = sso.consume(&caller, session.id, "wrong-token").await;
    assert!(matches!(
        res,
        Err(openpanel_domain::DbPrivilegeError::InvalidSsoToken)
    ));
}

#[tokio::test]
async fn grant_validation_rejects_unsafe_table_names() {
    let bad = make_grant(
        Uuid::new_v4(),
        Uuid::new_v4(),
        GrantScope::Table {
            name: "1starts_with_digit".into(),
        },
    );
    assert!(bad.validate().is_err());
    let bad = make_grant(
        Uuid::new_v4(),
        Uuid::new_v4(),
        GrantScope::Table {
            name: "drop-table".into(),
        },
    );
    assert!(bad.validate().is_err());
    let good = make_grant(
        Uuid::new_v4(),
        Uuid::new_v4(),
        GrantScope::Table {
            name: "users".into(),
        },
    );
    assert!(good.validate().is_ok());
}

#[tokio::test]
async fn list_user_grants_filters_by_user() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteDbPrivilegeRepository::new(db.pool()));
    let service = PrivilegeService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let database_id = Uuid::new_v4();
    let alice = Uuid::new_v4();
    let bob = Uuid::new_v4();
    service
        .apply(
            &caller,
            make_grant(database_id, alice, GrantScope::Database),
        )
        .await
        .expect("apply alice");
    service
        .apply(&caller, make_grant(database_id, bob, GrantScope::Database))
        .await
        .expect("apply bob");
    let alice_grants = service
        .list_user(&caller, database_id, alice)
        .await
        .expect("alice");
    assert_eq!(alice_grants.len(), 1);
    let bob_grants = service
        .list_user(&caller, database_id, bob)
        .await
        .expect("bob");
    assert_eq!(bob_grants.len(), 1);
    assert_ne!(alice_grants[0].id, bob_grants[0].id);
}
