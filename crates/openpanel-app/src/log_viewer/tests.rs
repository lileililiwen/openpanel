//! Log viewer bounded context unit and service tests.

use std::sync::Arc;

use chrono::{TimeZone, Utc};
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    LogAuthorization, LogDownloadRepository, LogLine, LogQuery, LogReader, LogSource,
    LogViewerError, RbacLogAuthorization, Role,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::log_viewer::{InMemoryLogReader, LogAggregator, SqliteLogViewerRepository};

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

fn owner_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("owner").expect("static"),
        Email::new("owner@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::Owner,
    )
}

fn regular_user() -> openpanel_domain::User {
    use openpanel_domain::{Email, Password, Username};
    openpanel_domain::User::new(
        Uuid::new_v4(),
        Username::new("alice").expect("static"),
        Email::new("alice@example.com").expect("static"),
        Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    )
}

fn line(ts_seconds: i64, service: &str, level: &str, msg: &str) -> LogLine {
    LogLine {
        ts: Utc.timestamp_opt(ts_seconds, 0).single().expect("ts"),
        source: LogSource::Site,
        service: service.into(),
        level: level.into(),
        message: msg.into(),
        fields: serde_json::Value::Null,
    }
}

#[tokio::test]
async fn rbac_owner_can_read_audit_log() {
    let auth = RbacLogAuthorization::new();
    let q = LogQuery::tail(LogSource::Audit, "", 10);
    assert!(auth.authorize(&owner_user(), &q).is_ok());
}

#[tokio::test]
async fn rbac_regular_user_cannot_read_audit_log() {
    let auth = RbacLogAuthorization::new();
    let q = LogQuery::tail(LogSource::Audit, "", 10);
    assert!(matches!(
        auth.authorize(&regular_user(), &q),
        Err(LogViewerError::Forbidden)
    ));
}

#[tokio::test]
async fn rbac_regular_user_cannot_read_system_log() {
    let auth = RbacLogAuthorization::new();
    let q = LogQuery::tail(LogSource::System, "nginx", 10);
    assert!(matches!(
        auth.authorize(&regular_user(), &q),
        Err(LogViewerError::Forbidden)
    ));
}

#[tokio::test]
async fn rbac_admin_can_read_system_log() {
    let auth = RbacLogAuthorization::new();
    let q = LogQuery::tail(LogSource::System, "nginx", 10);
    assert!(auth.authorize(&admin_user(), &q).is_ok());
}

#[tokio::test]
async fn reader_tail_returns_last_n_lines() {
    let reader = InMemoryLogReader::new();
    for i in 0..20 {
        reader.push(line(1_700_000_000 + i, "site-a", "info", &format!("m{i}")));
    }
    let q = LogQuery::tail(LogSource::Site, "site-a", 5);
    let page = reader.query(&q).await.expect("query");
    assert_eq!(page.lines.len(), 5);
    assert_eq!(page.lines.first().unwrap().message, "m15");
    assert_eq!(page.lines.last().unwrap().message, "m19");
    assert_eq!(page.total_scanned, 20);
}

#[tokio::test]
async fn reader_filter_narrows_results() {
    let reader = InMemoryLogReader::new();
    reader.push(line(1_700_000_000, "site-a", "info", "ok"));
    reader.push(line(1_700_000_001, "site-a", "error", "boom"));
    reader.push(line(1_700_000_002, "site-a", "info", "ok again"));
    let q = LogQuery {
        source: LogSource::Site,
        service: "site-a".into(),
        range: Default::default(),
        filter: Some("error".into()),
    };
    let page = reader.query(&q).await.expect("query");
    assert_eq!(page.lines.len(), 1);
    assert_eq!(page.lines[0].level, "error");
}

#[tokio::test]
async fn reader_filters_by_source() {
    let reader = InMemoryLogReader::new();
    reader.push(LogLine {
        ts: Utc::now(),
        source: LogSource::Site,
        service: "site-a".into(),
        level: "info".into(),
        message: "site".into(),
        fields: serde_json::Value::Null,
    });
    reader.push(LogLine {
        ts: Utc::now(),
        source: LogSource::Audit,
        service: "audit".into(),
        level: "info".into(),
        message: "audit".into(),
        fields: serde_json::Value::Null,
    });
    let q = LogQuery::tail(LogSource::Audit, "", 10);
    let page = reader.query(&q).await.expect("query");
    assert_eq!(page.lines.len(), 1);
    assert_eq!(page.lines[0].message, "audit");
}

#[tokio::test]
async fn aggregator_view_authorizes_and_queries() {
    let db = TestDb::new().await;
    let reader = Arc::new(InMemoryLogReader::new());
    reader.push(line(1_700_000_000, "site-a", "info", "hello"));
    let repo = Arc::new(SqliteLogViewerRepository::new(db.pool()));
    let agg = LogAggregator::new(reader, repo, Arc::new(NoopAuditService));
    let q = LogQuery::tail(LogSource::Site, "site-a", 10);
    let page = agg.view(&owner_user(), &q).await.expect("view");
    assert_eq!(page.lines.len(), 1);
}

#[tokio::test]
async fn aggregator_refuses_unauthorised_audit_view() {
    let db = TestDb::new().await;
    let reader = Arc::new(InMemoryLogReader::new());
    let repo = Arc::new(SqliteLogViewerRepository::new(db.pool()));
    let agg = LogAggregator::new(reader, repo, Arc::new(NoopAuditService));
    let q = LogQuery::tail(LogSource::Audit, "", 10);
    let res = agg.view(&regular_user(), &q).await;
    assert!(matches!(res, Err(LogViewerError::Forbidden)));
}

#[tokio::test]
async fn aggregator_download_records_audit_and_history() {
    let db = TestDb::new().await;
    let reader = Arc::new(InMemoryLogReader::new());
    reader.push(line(1_700_000_000, "site-a", "info", "hello"));
    let repo = Arc::new(SqliteLogViewerRepository::new(db.pool()));
    let agg = LogAggregator::new(reader, repo.clone(), Arc::new(NoopAuditService));
    let q = LogQuery::tail(LogSource::Site, "site-a", 10);
    let page = agg.download(&owner_user(), &q).await.expect("download");
    assert_eq!(page.lines.len(), 1);
    let history = repo.list_downloads(10).await.expect("history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].source, LogSource::Site);
    assert_eq!(history[0].line_count, 1);
}

#[tokio::test]
async fn log_query_default_tail_is_100() {
    let q = LogQuery::tail(LogSource::Site, "x", 5);
    assert_eq!(q.range.tail, Some(5));
    let default = LogQuery {
        source: LogSource::Site,
        service: "x".into(),
        range: Default::default(),
        filter: None,
    };
    assert_eq!(default.range.tail, Some(100));
}
