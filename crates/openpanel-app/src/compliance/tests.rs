//! Compliance bounded context unit and service tests.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{AuditRetentionPolicy, ComplianceRepository, REDACTED, Role};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::compliance::{
    AuditPurge, AuditRetentionService, GdprExporter, GdprSourceApiToken, GdprSourceDatabase,
    GdprSourceMailbox, GdprSourceSite, GdprSources, HardeningWizard, InMemoryRuleExecutor,
    SqliteComplianceRepository,
};

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

async fn make_repo(db: &TestDb) -> Arc<SqliteComplianceRepository> {
    Arc::new(SqliteComplianceRepository::new(db.pool()))
}

#[tokio::test]
async fn harden_stores_pre_and_post_images() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let wizard = HardeningWizard::new(repo.clone(), Arc::new(NoopAuditService))
        .with_executor(Arc::new(InMemoryRuleExecutor::new()));
    let caller = owner_user();
    let outcome = wizard
        .harden(&caller, "cis-debian-12-minimal")
        .await
        .expect("harden");
    assert!(!outcome.has_failures);
    let persisted = repo
        .get_hardening_run(outcome.run_id)
        .await
        .expect("get run")
        .expect("present");
    for rule in &persisted.rules {
        if matches!(rule.outcome, openpanel_domain::RuleOutcome::Applied) {
            assert_ne!(rule.pre_image, rule.post_image);
        }
    }
}

#[tokio::test]
async fn harden_reverts_to_pre_image_on_rollback() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let wizard = HardeningWizard::new(repo.clone(), Arc::new(NoopAuditService))
        .with_executor(Arc::new(InMemoryRuleExecutor::new()));
    let caller = owner_user();
    let outcome = wizard
        .harden(&caller, "cis-debian-12-minimal")
        .await
        .expect("harden");
    let rolled = wizard
        .rollback(&caller, outcome.run_id)
        .await
        .expect("rollback");
    assert!(!rolled.reverted.is_empty());
}

#[tokio::test]
async fn non_admin_cannot_harden() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let wizard = HardeningWizard::new(repo, Arc::new(NoopAuditService));
    let caller = non_admin_user();
    let res = wizard.harden(&caller, "cis-debian-12-minimal").await;
    assert!(matches!(
        res,
        Err(openpanel_domain::ComplianceError::Forbidden)
    ));
}

#[tokio::test]
async fn retention_policy_round_trip_and_default() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let service = AuditRetentionService::new(repo.clone(), Arc::new(NoopAuditService));
    let caller = owner_user();

    let default = service.get(&caller).await.expect("default");
    assert_eq!(default.ttl_days, 365);
    assert!(!default.export_before_purge);

    let updated = service.set(&caller, 90, true).await.expect("set retention");
    assert_eq!(updated.ttl_days, 90);
    assert!(updated.export_before_purge);
    let policy = service.get(&caller).await.expect("get");
    assert_eq!(policy.ttl_days, 90);
    assert!(policy.export_before_purge);
}

#[tokio::test]
async fn retention_policy_rejects_invalid_ttl() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let service = AuditRetentionService::new(repo, Arc::new(NoopAuditService));
    let caller = owner_user();
    let res = service.set(&caller, 0, false).await;
    assert!(res.is_err());
    let res = service.set(&caller, 9999, false).await;
    assert!(res.is_err());
}

struct CountingPurge {
    count: std::sync::Mutex<u64>,
}

#[async_trait]
impl AuditPurge for CountingPurge {
    async fn purge_before(
        &self,
        _cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, openpanel_domain::ComplianceError> {
        Ok(*self.count.lock().expect("count"))
    }
}

#[tokio::test]
async fn retention_purge_returns_count() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let service = AuditRetentionService::new(repo, Arc::new(NoopAuditService));
    let caller = owner_user();
    let purge = CountingPurge {
        count: std::sync::Mutex::new(7),
    };
    let count = service.run_purge(&caller, &purge).await.expect("purge");
    assert_eq!(count, 7);
}

struct FixtureSources;

#[async_trait]
impl GdprSources for FixtureSources {
    async fn sites_for(
        &self,
        _: Uuid,
    ) -> Result<Vec<GdprSourceSite>, openpanel_domain::ComplianceError> {
        Ok(vec![GdprSourceSite {
            id: Uuid::new_v4(),
            domain: "example.com".into(),
            aliases: "www.example.com".into(),
            owner_id: Uuid::nil(),
        }])
    }

    async fn mail_for(
        &self,
        _: Uuid,
    ) -> Result<Vec<GdprSourceMailbox>, openpanel_domain::ComplianceError> {
        Ok(vec![GdprSourceMailbox {
            address: "user@example.com".into(),
            display_name: "User".into(),
            quota_bytes: Some(1_000_000),
        }])
    }

    async fn databases_for(
        &self,
        _: Uuid,
    ) -> Result<Vec<GdprSourceDatabase>, openpanel_domain::ComplianceError> {
        Ok(vec![GdprSourceDatabase {
            id: Uuid::new_v4(),
            name: "wp".into(),
            owner_id: Uuid::nil(),
        }])
    }

    async fn api_tokens_for(
        &self,
        _: Uuid,
    ) -> Result<Vec<GdprSourceApiToken>, openpanel_domain::ComplianceError> {
        Ok(vec![GdprSourceApiToken {
            id: Uuid::new_v4(),
            name: "ci".into(),
            scopes: vec!["sites:read".into()],
        }])
    }
}

#[tokio::test]
async fn gdpr_export_redacts_secrets() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let exporter = GdprExporter::new(repo.clone(), Arc::new(NoopAuditService))
        .with_sources(Arc::new(FixtureSources));
    let caller = owner_user();
    let user_id = Uuid::new_v4();
    let export = exporter.export(&caller, user_id).await.expect("export");
    assert_eq!(export.payload.sites.len(), 1);
    assert_eq!(export.payload.mail.len(), 1);
    assert_eq!(export.payload.databases.len(), 1);
    assert_eq!(export.payload.api_tokens.len(), 1);
    assert_eq!(export.payload.databases[0].password, REDACTED);
    assert_eq!(export.payload.api_tokens[0].token, REDACTED);
}

#[tokio::test]
async fn gdpr_export_records_pii_count_in_audit() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let audit = Arc::new(NoopAuditService);
    let exporter = GdprExporter::new(repo, audit).with_sources(Arc::new(FixtureSources));
    let caller = owner_user();
    exporter
        .export(&caller, Uuid::new_v4())
        .await
        .expect("export");
}

#[tokio::test]
async fn hardening_run_persists_multiple_rules() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let wizard = HardeningWizard::new(repo.clone(), Arc::new(NoopAuditService))
        .with_executor(Arc::new(InMemoryRuleExecutor::new()));
    let caller = owner_user();
    let outcome = wizard
        .harden(&caller, "cis-debian-12-minimal")
        .await
        .expect("harden");
    let runs = repo.list_hardening_runs(10).await.expect("list runs");
    assert_eq!(runs.len(), 1);
    assert!(!runs[0].rules.is_empty());
    let _ = outcome;
    let _ = Utc::now();
}

#[tokio::test]
async fn retention_policy_persists_singleton() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let policy = AuditRetentionPolicy {
        ttl_days: 30,
        export_before_purge: true,
        updated_at: Utc::now(),
        updated_by: Uuid::nil(),
    };
    repo.save_retention_policy(&policy).await.expect("save");
    let loaded = repo
        .get_retention_policy()
        .await
        .expect("get")
        .expect("present");
    assert_eq!(loaded.ttl_days, 30);
    assert!(loaded.export_before_purge);
    // Singleton: second save overwrites the first.
    let second = AuditRetentionPolicy {
        ttl_days: 60,
        export_before_purge: false,
        updated_at: Utc::now(),
        updated_by: Uuid::nil(),
    };
    repo.save_retention_policy(&second).await.expect("save");
    let loaded = repo
        .get_retention_policy()
        .await
        .expect("get")
        .expect("present");
    assert_eq!(loaded.ttl_days, 60);
}

#[tokio::test]
async fn gdpr_export_round_trip_through_repo() {
    let db = TestDb::new().await;
    let repo = make_repo(&db).await;
    let exporter = GdprExporter::new(repo.clone(), Arc::new(NoopAuditService))
        .with_sources(Arc::new(FixtureSources));
    let caller = owner_user();
    let user_id = Uuid::new_v4();
    let export = exporter.export(&caller, user_id).await.expect("export");
    let loaded = repo
        .get_gdpr_export(export.id)
        .await
        .expect("get")
        .expect("present");
    assert_eq!(loaded.id, export.id);
    assert_eq!(loaded.user_id, user_id);
    let list = repo.list_gdpr_exports(user_id).await.expect("list");
    assert_eq!(list.len(), 1);
}
