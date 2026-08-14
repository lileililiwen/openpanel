//! Service manager bounded context unit and service tests.

use std::sync::Arc;

use openpanel_core::NoopAuditService;
use openpanel_domain::{Role, ServiceAction, ServiceManagerRepository, ServiceStatus};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::service_manager::{
    RecordingSystemCtl, ServiceActor, ServiceLister, SystemCtl,
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

#[tokio::test]
async fn list_returns_allowlisted_services_with_status() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    systemctl.set_active("nginx", ServiceStatus::Active);
    systemctl.set_enabled("nginx", true);
    let lister = ServiceLister::new(systemctl);
    let services = lister.list().await.expect("list");
    let nginx = services.iter().find(|s| s.name == "nginx").expect("nginx");
    assert_eq!(nginx.status, ServiceStatus::Active);
    assert!(nginx.enabled);
    assert!(!nginx.recent_logs.is_empty());
}

#[tokio::test]
async fn action_runs_through_executor_and_audits() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    systemctl.set_active("nginx", ServiceStatus::Inactive);
    let actor = ServiceActor::new(
        Arc::new(crate::service_manager::SqliteServiceManagerRepository::new(db.pool())),
        Arc::new(NoopAuditService),
        systemctl.clone(),
    );
    let caller = admin_user();
    let record = actor
        .act(&caller, "nginx", ServiceAction::Restart)
        .await
        .expect("restart");
    assert!(record.success);
    assert_eq!(record.action, ServiceAction::Restart);
    let calls = systemctl.calls();
    assert_eq!(calls, vec![("nginx".to_string(), ServiceAction::Restart)]);
}

#[tokio::test]
async fn action_rejects_unknown_unit() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    let actor = ServiceActor::new(
        Arc::new(crate::service_manager::SqliteServiceManagerRepository::new(db.pool())),
        Arc::new(NoopAuditService),
        systemctl.clone(),
    );
    let caller = admin_user();
    let res = actor.act(&caller, "rogue", ServiceAction::Restart).await;
    assert!(matches!(
        res,
        Err(openpanel_domain::ServiceError::NotAllowed(_))
    ));
    assert!(systemctl.calls().is_empty());
}

#[tokio::test]
async fn non_admin_cannot_act() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    let actor = ServiceActor::new(
        Arc::new(crate::service_manager::SqliteServiceManagerRepository::new(db.pool())),
        Arc::new(NoopAuditService),
        systemctl,
    );
    let caller = non_admin_user();
    let res = actor.act(&caller, "nginx", ServiceAction::Start).await;
    assert!(matches!(res, Err(openpanel_domain::ServiceError::Forbidden)));
}

#[tokio::test]
async fn action_verb_validation_only_accepts_five() {
    use openpanel_domain::ServiceAction as A;
    assert_eq!(A::from_verb("start"), Some(A::Start));
    assert_eq!(A::from_verb("stop"), Some(A::Stop));
    assert_eq!(A::from_verb("restart"), Some(A::Restart));
    assert_eq!(A::from_verb("enable"), Some(A::Enable));
    assert_eq!(A::from_verb("disable"), Some(A::Disable));
    assert_eq!(A::from_verb("status"), None);
    assert_eq!(A::from_verb("reload"), None);
    assert_eq!(A::from_verb("rm"), None);
    assert_eq!(A::from_verb(""), None);
}

#[tokio::test]
async fn status_maps_systemd_strings() {
    use openpanel_domain::ServiceStatus as S;
    assert_eq!(S::from_systemd("active"), S::Active);
    assert_eq!(S::from_systemd("inactive"), S::Inactive);
    assert_eq!(S::from_systemd("deactivating"), S::Inactive);
    assert_eq!(S::from_systemd("failed"), S::Failed);
    assert_eq!(S::from_systemd("reloading"), S::Unknown);
    assert_eq!(S::from_systemd("auto-restart"), S::Unknown);
}

#[tokio::test]
async fn history_is_persisted() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    let repo = Arc::new(crate::service_manager::SqliteServiceManagerRepository::new(
        db.pool(),
    ));
    let actor = ServiceActor::new(repo.clone(), Arc::new(NoopAuditService), systemctl);
    let caller = admin_user();
    actor.act(&caller, "nginx", ServiceAction::Start).await.expect("start");
    actor.act(&caller, "redis", ServiceAction::Stop).await.expect("stop");
    let history = repo.list_actions(10).await.expect("history");
    assert_eq!(history.len(), 2);
    let nginx = repo.list_actions_for("nginx", 10).await.expect("nginx");
    assert_eq!(nginx.len(), 1);
    assert_eq!(nginx[0].action, ServiceAction::Start);
}

#[tokio::test]
async fn enable_disable_toggle_unit_boot_state() {
    let db = TestDb::new().await;
    let systemctl = Arc::new(RecordingSystemCtl::new());
    let actor = ServiceActor::new(
        Arc::new(crate::service_manager::SqliteServiceManagerRepository::new(db.pool())),
        Arc::new(NoopAuditService),
        systemctl.clone(),
    );
    let caller = admin_user();
    actor.act(&caller, "nginx", ServiceAction::Enable).await.expect("enable");
    assert!(systemctl.is_enabled("nginx").await.expect("enabled"));
    actor.act(&caller, "nginx", ServiceAction::Disable).await.expect("disable");
    assert!(!systemctl.is_enabled("nginx").await.expect("disabled"));
}
