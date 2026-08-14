//! Synthetic monitoring bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    CheckStatus, CheckType, Role, SyntheticCheck, SyntheticRepository,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::synthetic_monitoring::{
    CheckRunner, ProbeScheduler, RecordingProbe, SqliteSyntheticRepository,
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

fn make_check(kind: CheckType, target: &str) -> SyntheticCheck {
    SyntheticCheck {
        id: Uuid::new_v4(),
        name: "test".into(),
        kind,
        target: target.into(),
        expected_status: Some(200),
        timeout_secs: 5,
        throttle_secs: 0,
        warn_before_days: 14,
        created_at: Utc::now(),
        last_run_at: None,
        enabled: true,
    }
}

#[tokio::test]
async fn http_check_runs_and_records_result() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_http_response(200, 42);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let check = make_check(CheckType::Http, "https://example.com");
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Ok);
    assert_eq!(result.http_status, Some(200));
    assert_eq!(result.latency_ms, 42);
    let runs = repo.list_results(check.id, 10).await.expect("runs");
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].status, CheckStatus::Ok);
}

#[tokio::test]
async fn http_check_with_5xx_is_fail() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_http_response(503, 17);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let check = make_check(CheckType::Http, "https://example.com");
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Fail);
}

#[tokio::test]
async fn http_check_with_4xx_is_warn() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_http_response(404, 17);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let check = make_check(CheckType::Http, "https://example.com");
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Warn);
}

#[tokio::test]
async fn ssl_check_inside_warn_window_raises_warn() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_ssl_days_remaining(7);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let mut check = make_check(CheckType::Ssl, "example.com");
    check.warn_before_days = 14;
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Warn);
    assert_eq!(result.cert_days_remaining, Some(7));
}

#[tokio::test]
async fn ssl_check_with_zero_days_remaining_is_fail() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_ssl_days_remaining(0);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let check = make_check(CheckType::Ssl, "example.com");
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Fail);
}

#[tokio::test]
async fn tcp_check_succeeds_within_timeout() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_tcp_latency(12);
    let runner = CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    );
    let check = make_check(CheckType::Tcp, "example.com:443");
    repo.save_check(&check).await.expect("save");
    let result = runner.run(&check).await.expect("run");
    assert_eq!(result.status, CheckStatus::Ok);
    assert_eq!(result.latency_ms, 12);
    assert_eq!(probe.tcp_calls(), vec![("example.com".to_string(), 443)]);
}

#[tokio::test]
async fn scheduler_throttles_forced_runs() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    probe.set_http_response(200, 1);
    let runner = Arc::new(CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    ));
    let scheduler = ProbeScheduler::new(repo.clone(), runner);
    let mut check = make_check(CheckType::Http, "https://example.com");
    check.throttle_secs = 60;
    repo.save_check(&check).await.expect("save");
    let first = scheduler.force_run(&admin_user(), check.id).await.expect("first");
    assert_eq!(first.status, CheckStatus::Ok);
    let second = scheduler.force_run(&admin_user(), check.id).await;
    assert!(matches!(
        second,
        Err(openpanel_domain::SyntheticError::Throttled)
    ));
    let history = repo.list_results(check.id, 10).await.expect("history");
    assert_eq!(history.len(), 1);
}

#[tokio::test]
async fn non_admin_cannot_force_run() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let probe = Arc::new(RecordingProbe::new());
    let runner = Arc::new(CheckRunner::new(
        repo.clone(),
        probe.clone(),
        probe.clone(),
        probe.clone(),
        Arc::new(NoopAuditService),
    ));
    let scheduler = ProbeScheduler::new(repo.clone(), runner);
    let check = make_check(CheckType::Http, "https://example.com");
    repo.save_check(&check).await.expect("save");
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = scheduler.force_run(&user, check.id).await;
    assert!(matches!(res, Err(openpanel_domain::SyntheticError::Forbidden)));
}

#[tokio::test]
async fn check_validation_rejects_malformed_targets() {
    let mut check = make_check(CheckType::Http, "");
    assert!(check.validate().is_err());
    check.target = "ftp://example.com".into();
    assert!(check.validate().is_err());
    check.target = "https://example.com".into();
    assert!(check.validate().is_ok());
    let mut check = make_check(CheckType::Tcp, "example.com");
    assert!(check.validate().is_err());
    check.target = "example.com:443".into();
    assert!(check.validate().is_ok());
    let mut check = make_check(CheckType::Ssl, "example.com/path");
    assert!(check.validate().is_err());
    check.target = "example.com".into();
    assert!(check.validate().is_ok());
}

#[tokio::test]
async fn touch_updates_last_run_at() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteSyntheticRepository::new(db.pool()));
    let check = make_check(CheckType::Http, "https://example.com");
    repo.save_check(&check).await.expect("save");
    let now = Utc::now();
    repo.touch_check(check.id, now).await.expect("touch");
    let loaded = repo.get_check(check.id).await.expect("get").expect("present");
    assert_eq!(loaded.last_run_at, Some(now));
}
