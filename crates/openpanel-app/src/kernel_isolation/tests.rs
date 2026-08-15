//! Kernel isolation bounded context unit and service tests.

use std::sync::Arc;

use openpanel_core::NoopAuditService;
use openpanel_domain::{
    CgroupLimit, IsolationError, IsolationPolicy, IsolationRepository, Role,
    UserNamespaceConfig, user_cgroup_path,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::kernel_isolation::{
    CgroupEnforcer, CgroupWriter, NamespaceIsolator, QuotaBridge, RecordingCgroupWriter,
    SqliteIsolationRepository,
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

#[test]
fn user_cgroup_path_is_namespaced_under_root() {
    let id = Uuid::new_v4();
    let path = user_cgroup_path(id);
    assert!(path.starts_with("/sys/fs/cgroup/openpanel/"));
    assert!(path.contains(&id.to_string()));
}

#[test]
fn default_limit_is_sane() {
    let id = Uuid::new_v4();
    let limit = CgroupLimit::default_for(id);
    assert!(limit.validate().is_ok());
    assert!(limit.cpu_millicores > 0);
    assert!(limit.pids_max > 0);
}

#[test]
fn quota_bridge_builds_policy_from_plan() {
    let admin = Uuid::new_v4();
    let policy = QuotaBridge::from_plan_quota(2000, 2048, 4096, 256, admin);
    assert_eq!(policy.default_cpu_millicores, 2000);
    assert_eq!(policy.default_pids_max, 256);
    assert_eq!(policy.updated_by, admin);
}

#[tokio::test]
async fn enforcer_writes_three_keys_per_apply() {
    let writer = Arc::new(RecordingCgroupWriter::new());
    let enforcer = CgroupEnforcer::new(writer.clone(), Arc::new(NoopAuditService));
    let caller = admin_user();
    let user_id = Uuid::new_v4();
    let limit = CgroupLimit {
        user_id,
        cpu_millicores: 1000,
        memory_high_mib: 1024,
        memory_max_mib: 2048,
        pids_max: 256,
        updated_at: chrono::Utc::now(),
    };
    enforcer.apply(&caller, &limit).await.expect("apply");
    let calls = writer.calls();
    assert_eq!(calls.len(), 4);
    let paths: Vec<_> = calls.iter().map(|(p, _, _)| p.clone()).collect();
    let expected = user_cgroup_path(user_id);
    for path in &paths {
        assert_eq!(path, &expected);
    }
    let keys: std::collections::HashSet<_> =
        calls.iter().map(|(_, k, _)| k.clone()).collect();
    assert!(keys.contains("cpu.max"));
    assert!(keys.contains("memory.high"));
    assert!(keys.contains("memory.max"));
    assert!(keys.contains("pids.max"));
}

#[tokio::test]
async fn enforcer_keeps_isolation_on_partial_failure() {
    struct FlakyWriter;
    #[async_trait::async_trait]
    impl CgroupWriter for FlakyWriter {
        fn write(&self, path: &str, key: &str, _value: u64) -> Result<String, IsolationError> {
            if key == "pids.max" {
                Err(IsolationError::CgroupWrite(path.to_string()))
            } else {
                Ok(format!("{path}/{key}"))
            }
        }
        fn read(&self, _path: &str, _key: &str) -> Result<u64, IsolationError> {
            Ok(0)
        }
    }
    let enforcer = CgroupEnforcer::new(Arc::new(FlakyWriter), Arc::new(NoopAuditService));
    let caller = admin_user();
    let limit = CgroupLimit {
        user_id: Uuid::new_v4(),
        cpu_millicores: 1000,
        memory_high_mib: 1024,
        memory_max_mib: 2048,
        pids_max: 256,
        updated_at: chrono::Utc::now(),
    };
    let res = enforcer.apply(&caller, &limit).await;
    assert!(matches!(res, Err(IsolationError::CgroupWrite(_))));
}

#[tokio::test]
async fn non_admin_cannot_apply() {
    let writer = Arc::new(RecordingCgroupWriter::new());
    let enforcer = CgroupEnforcer::new(writer, Arc::new(NoopAuditService));
    let caller = non_admin_user();
    let limit = CgroupLimit {
        user_id: Uuid::new_v4(),
        cpu_millicores: 1000,
        memory_high_mib: 1024,
        memory_max_mib: 2048,
        pids_max: 256,
        updated_at: chrono::Utc::now(),
    };
    let res = enforcer.apply(&caller, &limit).await;
    assert!(matches!(res, Err(IsolationError::Forbidden)));
}

#[tokio::test]
async fn repo_round_trips_limit_and_namespace_and_policy() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteIsolationRepository::new(db.pool()));
    let user_id = Uuid::new_v4();
    let limit = CgroupLimit::default_for(user_id);
    repo.save_limit(&limit).await.expect("save limit");
    let loaded = repo.get_limit(user_id).await.expect("get").expect("present");
    assert_eq!(loaded.user_id, user_id);
    let ns = UserNamespaceConfig::default_for(user_id);
    repo.save_namespace(&ns).await.expect("save ns");
    let loaded_ns = repo.get_namespace(user_id).await.expect("get ns").expect("present");
    assert!(loaded_ns.enabled);
    let policy = IsolationPolicy::default();
    repo.save_policy(&policy).await.expect("save policy");
    let loaded_policy = repo.get_policy().await.expect("get policy").expect("present");
    assert_eq!(loaded_policy.default_pids_max, policy.default_pids_max);
}

#[test]
fn namespace_isolator_accepts_enabled_config() {
    let config = UserNamespaceConfig::default_for(Uuid::new_v4());
    assert!(NamespaceIsolator::new().apply(&config).is_ok());
}

#[test]
fn namespace_isolator_noops_when_disabled() {
    let mut config = UserNamespaceConfig::default_for(Uuid::new_v4());
    config.enabled = false;
    assert!(NamespaceIsolator::new().apply(&config).is_ok());
}

#[test]
fn limit_validation_rejects_zero_cpu() {
    let mut limit = CgroupLimit::default_for(Uuid::new_v4());
    limit.cpu_millicores = 0;
    assert!(limit.validate().is_err());
}

#[test]
fn limit_validation_rejects_high_le_max() {
    let mut limit = CgroupLimit::default_for(Uuid::new_v4());
    limit.memory_high_mib = 4096;
    limit.memory_max_mib = 1024;
    assert!(limit.validate().is_err());
}