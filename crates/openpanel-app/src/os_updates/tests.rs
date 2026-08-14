//! OS update management bounded context unit and service tests.

use std::sync::Arc;

use openpanel_core::NoopAuditService;
use openpanel_domain::{OsUpdateRepository, RebootState, Role, UpdateKind, UpdatePolicy};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::os_updates::{
    OsUpdateApplier, OsUpdateLister, RecordingPackageManager, SqliteOsUpdateRepository,
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
async fn lister_parses_security_and_other_updates() {
    let pm = Arc::new(RecordingPackageManager::new());
    let output = "Inst libssl3 [3.0.11-1] (3.0.13-1 Debian:12.4/security [amd64])\n\
                  Inst bash [5.2.15-2] (5.2.15-2+b1 Debian:12.4 [amd64])\n";
    pm.set_simulate_output(output);
    let lister = OsUpdateLister::new(pm);
    let updates = lister.list().await.expect("list");
    assert_eq!(updates.len(), 2);
    assert_eq!(updates[0].kind, UpdateKind::Security);
    assert_eq!(updates[1].kind, UpdateKind::Other);
}

#[tokio::test]
async fn applier_records_history_and_audits() {
    let db = TestDb::new().await;
    let pm = Arc::new(RecordingPackageManager::new());
    pm.set_simulate_output("Inst libssl3 [3.0.11] (3.0.13 Debian:12.4/security [amd64])\n");
    pm.set_kernel_updated(true);
    let repo = Arc::new(SqliteOsUpdateRepository::new(db.pool()));
    let applier = OsUpdateApplier::new(repo.clone(), Arc::new(NoopAuditService), pm.clone());
    let caller = admin_user();
    let record = applier
        .apply(&caller, UpdateKind::Security)
        .await
        .expect("apply");
    assert!(record.success);
    assert_eq!(record.reboot, RebootState { required: true, kernel_updated: true });
    let calls = pm.apply_calls();
    assert_eq!(calls, vec![UpdateKind::Security]);
    let history = repo.list_history(10).await.expect("history");
    assert_eq!(history.len(), 1);
    assert!(history[0].reboot.required);
}

#[tokio::test]
async fn non_admin_cannot_apply() {
    let db = TestDb::new().await;
    let pm = Arc::new(RecordingPackageManager::new());
    let repo = Arc::new(SqliteOsUpdateRepository::new(db.pool()));
    let applier = OsUpdateApplier::new(repo, Arc::new(NoopAuditService), pm);
    let caller = non_admin_user();
    let res = applier.apply(&caller, UpdateKind::Security).await;
    assert!(matches!(res, Err(openpanel_domain::OsUpdateError::Forbidden)));
}

#[tokio::test]
async fn policy_round_trip_and_validation() {
    let db = TestDb::new().await;
    let repo = SqliteOsUpdateRepository::new(db.pool());
    let policy = UpdatePolicy {
        security_auto_install: true,
        other_auto_install: false,
        run_hour: 6,
        auto_reboot: true,
    };
    repo.save_policy(&policy).await.expect("save");
    let loaded = repo.get_policy().await.expect("get").expect("present");
    assert!(loaded.security_auto_install);
    assert!(!loaded.other_auto_install);
    assert_eq!(loaded.run_hour, 6);
    assert!(loaded.auto_reboot);
    let bad = UpdatePolicy {
        run_hour: 30,
        ..UpdatePolicy::default()
    };
    assert!(bad.validate().is_err());
}

#[tokio::test]
async fn policy_renders_unattended_upgrades_config() {
    let policy = UpdatePolicy {
        security_auto_install: true,
        other_auto_install: false,
        run_hour: 4,
        auto_reboot: true,
    };
    let body = policy.render_apt_config();
    assert!(body.contains("APT::Periodic::Unattended-Upgrade \"1\""));
    assert!(body.contains("Automatic-Reboot \"true\""));
    assert!(body.contains("Automatic-Reboot-Time \"04:00\""));
}

#[tokio::test]
async fn unattended_config_writer_persists_to_path() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("20auto-upgrades");
    let config = crate::os_updates::UnattendedConfig::new(path.clone());
    let bytes = config
        .write(&UpdatePolicy {
            run_hour: 5,
            ..UpdatePolicy::default()
        })
        .expect("write");
    assert!(bytes > 0);
    let body = std::fs::read_to_string(&path).expect("read");
    assert!(body.contains("APT::Periodic::Update-Package-Lists"));
    assert!(body.contains("Automatic-Reboot-Time \"05:00\""));
}

#[tokio::test]
async fn apply_records_failure_when_package_manager_errors() {
    struct FailingPm;
    #[async_trait::async_trait]
    impl crate::os_updates::PackageManager for FailingPm {
        async fn simulate_upgrade(&self) -> Result<crate::os_updates::CommandOutput, openpanel_domain::OsUpdateError> {
            Ok(crate::os_updates::CommandOutput {
                code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
        async fn apply_upgrade(&self, _: UpdateKind) -> Result<crate::os_updates::CommandOutput, openpanel_domain::OsUpdateError> {
            Ok(crate::os_updates::CommandOutput {
                code: 1,
                stdout: String::new(),
                stderr: "apt failed".into(),
            })
        }
        async fn kernel_updated(&self) -> Result<bool, openpanel_domain::OsUpdateError> {
            Ok(false)
        }
    }
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteOsUpdateRepository::new(db.pool()));
    let applier = OsUpdateApplier::new(repo.clone(), Arc::new(NoopAuditService), Arc::new(FailingPm));
    let caller = admin_user();
    let record = applier.apply(&caller, UpdateKind::Other).await.expect("apply");
    assert!(!record.success);
    assert_eq!(record.message, "apt failed");
    let history = repo.list_history(10).await.expect("history");
    assert_eq!(history.len(), 1);
    assert!(!history[0].success);
}
