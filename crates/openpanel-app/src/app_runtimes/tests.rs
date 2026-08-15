//! Non-PHP runtime bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{Role, RuntimeKind, RuntimeRepository, RuntimeStatus};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::app_runtimes::{
    ReverseProxyLayer, RuntimeService, SqliteRuntimeRepository, SupervisorUnitBuilder,
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

fn make_runtime() -> openpanel_domain::SiteRuntime {
    openpanel_domain::SiteRuntime {
        id: Uuid::new_v4(),
        site_id: Uuid::new_v4(),
        kind: RuntimeKind::Node,
        version: "20.10.0".into(),
        app_port: 3000,
        workdir: "app".into(),
        start_command: String::new(),
        registered_at: Utc::now(),
        status: RuntimeStatus::Stopped,
    }
}

#[tokio::test]
async fn set_runtime_persists_and_audits() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteRuntimeRepository::new(db.pool()));
    let service = RuntimeService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        SupervisorUnitBuilder::new(),
        ReverseProxyLayer::new(),
    );
    let caller = admin_user();
    let runtime = make_runtime();
    service
        .set(&caller, runtime.clone())
        .await
        .expect("set");
    let loaded = repo.get_runtime(runtime.site_id).await.expect("get").expect("present");
    assert_eq!(loaded.kind, RuntimeKind::Node);
    assert_eq!(loaded.version, "20.10.0");
}

#[tokio::test]
async fn set_runtime_rejects_invalid_config() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteRuntimeRepository::new(db.pool()));
    let service = RuntimeService::new(
        repo,
        Arc::new(NoopAuditService),
        SupervisorUnitBuilder::new(),
        ReverseProxyLayer::new(),
    );
    let caller = admin_user();
    let mut runtime = make_runtime();
    runtime.app_port = 80;
    let res = service.set(&caller, runtime).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::RuntimeError::PortNotAllowed(_))
    ));
}

#[tokio::test]
async fn non_admin_cannot_set_runtime() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteRuntimeRepository::new(db.pool()));
    let service = RuntimeService::new(
        repo,
        Arc::new(NoopAuditService),
        SupervisorUnitBuilder::new(),
        ReverseProxyLayer::new(),
    );
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = service.set(&user, make_runtime()).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::RuntimeError::Forbidden)
    ));
}

#[tokio::test]
async fn set_status_updates_runtime() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteRuntimeRepository::new(db.pool()));
    let service = RuntimeService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        SupervisorUnitBuilder::new(),
        ReverseProxyLayer::new(),
    );
    let caller = admin_user();
    let runtime = make_runtime();
    let site_id = runtime.site_id;
    service.set(&caller, runtime).await.expect("set");
    let after = service
        .set_status(&caller, site_id, RuntimeStatus::Running)
        .await
        .expect("status");
    assert_eq!(after.status, RuntimeStatus::Running);
    let after = service
        .set_status(&caller, site_id, RuntimeStatus::Stopped)
        .await
        .expect("status");
    assert_eq!(after.status, RuntimeStatus::Stopped);
}

#[tokio::test]
async fn supervisor_unit_uses_chroot_workdir() {
    let runtime = make_runtime();
    let unit = SupervisorUnitBuilder::new().build(&runtime, "site-user", "/srv/site");
    assert!(unit.contains("WorkingDirectory=/srv/site/app"));
    assert!(unit.contains("User=site-user"));
    assert!(unit.contains("APP_PORT=3000"));
}

#[tokio::test]
async fn reverse_proxy_targets_loopback() {
    let runtime = make_runtime();
    let block = ReverseProxyLayer::new().render(&runtime, "example.com");
    assert!(block.contains("proxy_pass http://127.0.0.1:3000"));
    assert!(!block.contains("0.0.0.0"));
}

#[tokio::test]
async fn validate_rejects_outside_chroot_and_bad_port() {
    let mut runtime = make_runtime();
    runtime.workdir = "../etc".into();
    assert!(matches!(
        runtime.validate(),
        Err(openpanel_domain::RuntimeError::OutsideChroot(_))
    ));
    let mut runtime = make_runtime();
    runtime.workdir = "/etc".into();
    assert!(matches!(
        runtime.validate(),
        Err(openpanel_domain::RuntimeError::OutsideChroot(_))
    ));
    let mut runtime = make_runtime();
    runtime.app_port = 22;
    assert!(matches!(
        runtime.validate(),
        Err(openpanel_domain::RuntimeError::PortNotAllowed(_))
    ));
}

#[tokio::test]
async fn list_returns_all_runtimes() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteRuntimeRepository::new(db.pool()));
    let service = RuntimeService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        SupervisorUnitBuilder::new(),
        ReverseProxyLayer::new(),
    );
    let caller = admin_user();
    let mut r1 = make_runtime();
    r1.app_port = 3000;
    service.set(&caller, r1).await.expect("set 1");
    let mut r2 = make_runtime();
    r2.app_port = 4000;
    service.set(&caller, r2).await.expect("set 2");
    let list = service.list().await.expect("list");
    assert_eq!(list.len(), 2);
}