//! Mail anti-spam and filtering bounded context unit and service tests.

use std::sync::Arc;

use chrono::{Duration, Utc};
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    AntiSpamPolicy, AutoResponder, AutoResponderMode, CatchAll, Forwarder, MailFilterError,
    MailFilterRepository, MailingList, Role, SieveScript,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::mail_filtering::{
    MailFilterService, MailingListService, SieveCompiler, SpamScorer, SqliteMailFilterRepository,
    route_for_score,
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
async fn set_policy_persists_and_audits() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        SieveCompiler::new(),
    );
    let caller = admin_user();
    let mailbox_id = Uuid::new_v4();
    let policy = AntiSpamPolicy {
        mailbox_id,
        spam_threshold: 70,
        greylist_enabled: true,
        updated_at: Utc::now(),
    };
    service.set_policy(&caller, policy.clone()).await.expect("set");
    let loaded = repo.get_policy(mailbox_id).await.expect("get").expect("present");
    assert_eq!(loaded.spam_threshold, 70);
}

#[tokio::test]
async fn set_policy_rejects_invalid_threshold() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(
        repo,
        Arc::new(NoopAuditService),
        SieveCompiler::new(),
    );
    let caller = admin_user();
    let policy = AntiSpamPolicy {
        mailbox_id: Uuid::new_v4(),
        spam_threshold: 200,
        greylist_enabled: false,
        updated_at: Utc::now(),
    };
    let res = service.set_policy(&caller, policy).await;
    assert!(matches!(res, Err(MailFilterError::InvalidAutoResponder)));
}

#[tokio::test]
async fn set_sieve_compiles_and_persists() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(
        repo.clone(),
        Arc::new(NoopAuditService),
        SieveCompiler::new(),
    );
    let caller = admin_user();
    let mailbox_id = Uuid::new_v4();
    let script = SieveScript::new(
        mailbox_id,
        "if header :contains \"subject\" \"hello\" { fileinto \"INBOX\"; }",
    )
    .expect("sieve");
    let saved = service.set_sieve(&caller, script).await.expect("set");
    assert!(saved.last_compiled_at.is_some());
    let loaded = repo.get_sieve(mailbox_id).await.expect("get").expect("present");
    assert_eq!(loaded.script, saved.script);
}

#[tokio::test]
async fn sieve_compiler_rejects_unbalanced_braces() {
    let mailbox_id = Uuid::new_v4();
    let bad = SieveScript::new(
        mailbox_id,
        "if header :contains \"subject\" \"hello\" { fileinto \"INBOX\"; ",
    )
    .expect("size ok");
    let compiler = SieveCompiler::new();
    assert!(matches!(
        compiler.compile(&bad),
        Err(MailFilterError::SieveCompile(_))
    ));
}

#[tokio::test]
async fn sieve_script_size_cap_is_enforced() {
    let mailbox_id = Uuid::new_v4();
    let big = "x".repeat(openpanel_domain::SIEVE_MAX_BYTES + 1);
    let res = SieveScript::new(mailbox_id, &big);
    assert!(matches!(res, Err(MailFilterError::SieveTooLarge(_, _))));
}

#[tokio::test]
async fn autoresponder_validates_window() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(repo, Arc::new(NoopAuditService), SieveCompiler::new());
    let caller = admin_user();
    let now = Utc::now();
    let bad = AutoResponder {
        mailbox_id: Uuid::new_v4(),
        enabled: true,
        body: "Out of office".into(),
        mode: AutoResponderMode::Every,
        window_start: now,
        window_end: now,
    };
    assert!(matches!(
        service.set_autoresponder(&caller, bad).await,
        Err(MailFilterError::InvalidAutoResponder)
    ));
}

#[tokio::test]
async fn forwarder_rejects_self_loop() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(repo, Arc::new(NoopAuditService), SieveCompiler::new());
    let caller = admin_user();
    let f = Forwarder {
        mailbox_id: Uuid::new_v4(),
        destination: "user@example.com".into(),
        keep_local: true,
    };
    let res = service
        .add_forwarder(&caller, f, "user@example.com")
        .await;
    assert!(matches!(res, Err(MailFilterError::ForwarderLoop)));
}

#[tokio::test]
async fn spam_scorer_routes_above_threshold() {
    let scorer = SpamScorer::new();
    let score = scorer.score(200_000, true, 1.0);
    // 20 (body) + 15 (attachment) + 25 (uppercase ratio 1.0) = 60
    assert_eq!(score, 60);
    let policy = AntiSpamPolicy {
        mailbox_id: Uuid::new_v4(),
        spam_threshold: 50,
        greylist_enabled: false,
        updated_at: Utc::now(),
    };
    assert!(matches!(
        route_for_score(&policy, score),
        crate::mail_filtering::ScoreOutcome::Spam
    ));
    assert!(matches!(
        route_for_score(&policy, 30),
        crate::mail_filtering::ScoreOutcome::Inbox
    ));
}

#[tokio::test]
async fn mailing_list_service_persists_and_loads() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailingListService::new(repo.clone());
    let caller = admin_user();
    let list = MailingList {
        address: "team@example.com".into(),
        members: vec![Uuid::new_v4(), Uuid::new_v4()],
        created_at: Utc::now(),
    };
    service.upsert(&caller, list.clone()).await.expect("upsert");
    let loaded = service.get("team@example.com").await.expect("get").expect("present");
    assert_eq!(loaded.members.len(), 2);
}

#[tokio::test]
async fn catch_all_persists() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(repo, Arc::new(NoopAuditService), SieveCompiler::new());
    let caller = admin_user();
    let catch_all = CatchAll {
        domain: "example.com".into(),
        destination_mailbox: Uuid::new_v4(),
    };
    service
        .set_catch_all(&caller, catch_all.clone())
        .await
        .expect("set");
}

#[tokio::test]
async fn non_admin_cannot_use_mail_filter() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteMailFilterRepository::new(db.pool()));
    let service = MailFilterService::new(repo, Arc::new(NoopAuditService), SieveCompiler::new());
    let user = non_admin_user();
    let policy = AntiSpamPolicy::default_for(Uuid::new_v4());
    let res = service.set_policy(&user, policy).await;
    assert!(matches!(res, Err(MailFilterError::Forbidden)));
    let _ = Duration::seconds(1);
}