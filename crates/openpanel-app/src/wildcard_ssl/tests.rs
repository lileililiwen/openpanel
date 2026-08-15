//! Wildcard SSL with DNS-01 bounded context unit and service tests.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::NoopAuditService;
use openpanel_domain::{
    AcmeEndpointMode, CertRequest, ChallengeKind, Role, WildcardRepository,
};
use openpanel_test_support::TestDb;
use uuid::Uuid;

use crate::wildcard_ssl::{
    CertRenewalScheduler, Dns01ChallengeSolver, SqliteWildcardRepository, WildcardIssuer,
};
use openpanel_domain::RecordingDnsProvider;

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

fn make_request(wildcard: bool) -> CertRequest {
    CertRequest::new(
        Uuid::new_v4(),
        "example.com",
        wildcard,
        if wildcard {
            ChallengeKind::Dns01
        } else {
            ChallengeKind::Http01
        },
        AcmeEndpointMode::Staging,
        "route53",
    )
    .expect("req")
}

#[tokio::test]
async fn solver_publishes_then_revokes_lease() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let dns = Arc::new(RecordingDnsProvider::new());
    let solver = Dns01ChallengeSolver::new(
        repo.clone(),
        dns.clone(),
        Arc::new(NoopAuditService),
    );
    let request = make_request(true);
    repo.save_request(&request).await.expect("save");
    solver
        .solve(&admin_user(), &request, |_value| async { Ok(()) })
        .await
        .expect("solve");
    let publishes = dns.publishes();
    let revokes = dns.revokes();
    assert_eq!(publishes.len(), 1);
    assert_eq!(publishes[0].0, "_acme-challenge.example.com");
    assert_eq!(revokes.len(), 1);
    assert_eq!(revokes[0], "_acme-challenge.example.com");
}

#[tokio::test]
async fn solver_revokes_lease_on_failure() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let dns = Arc::new(RecordingDnsProvider::new());
    let solver = Dns01ChallengeSolver::new(
        repo.clone(),
        dns.clone(),
        Arc::new(NoopAuditService),
    );
    let request = make_request(true);
    repo.save_request(&request).await.expect("save");
    let res = solver
        .solve(&admin_user(), &request, |_value| async {
            Err(openpanel_domain::WildcardError::ChallengeFailed("nope".into()))
        })
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::WildcardError::ChallengeFailed(_))
    ));
    // The lease was still revoked.
    assert_eq!(dns.revokes().len(), 1);
}

#[tokio::test]
async fn solver_rejects_disallowed_provider() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let dns = Arc::new(RecordingDnsProvider::new());
    let solver = Dns01ChallengeSolver::new(
        repo.clone(),
        dns.clone(),
        Arc::new(NoopAuditService),
    );
    let request = CertRequest::new(
        Uuid::new_v4(),
        "example.com",
        true,
        ChallengeKind::Dns01,
        AcmeEndpointMode::Staging,
        "rogue",
    )
    .expect("req");
    let res = solver
        .solve(&admin_user(), &request, |_| async { Ok(()) })
        .await;
    assert!(matches!(
        res,
        Err(openpanel_domain::WildcardError::ProviderNotAllowed(_))
    ));
    assert!(dns.publishes().is_empty());
}

#[tokio::test]
async fn issuer_persists_request_and_audits() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let dns = Arc::new(RecordingDnsProvider::new());
    let solver = Dns01ChallengeSolver::new(
        repo.clone(),
        dns.clone(),
        Arc::new(NoopAuditService),
    );
    let issuer = WildcardIssuer::new(
        repo.clone(),
        solver,
        Arc::new(NoopAuditService),
    );
    let request = make_request(true);
    let req_id = request.id;
    issuer
        .issue(&admin_user(), request.clone())
        .await
        .expect("issue");
    let loaded = repo.get_request(req_id).await.expect("get").expect("present");
    assert_eq!(loaded.apex, "example.com");
    assert!(loaded.wildcard);
    assert_eq!(loaded.dns_provider, "route53");
    assert_eq!(dns.publishes().len(), 1);
    assert_eq!(dns.revokes().len(), 1);
}

#[tokio::test]
async fn scheduler_returns_60_days_after_creation() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let scheduler = CertRenewalScheduler::new(repo, Arc::new(NoopAuditService));
    let request = make_request(true);
    let next = scheduler.next_renewal(&admin_user(), &request);
    let now = Utc::now();
    let diff = (next - now).num_days();
    assert!(diff >= 59 && diff <= 61);
}

#[tokio::test]
async fn lease_round_trip_through_repo() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let request = make_request(true);
    repo.save_request(&request).await.expect("save");
    let lease = openpanel_domain::DnsLease {
        id: Uuid::new_v4(),
        cert_request_id: request.id,
        fqdn: "_acme-challenge.example.com".into(),
        value: "token".into(),
        created_at: Utc::now(),
        revoked_at: None,
    };
    repo.save_lease(&lease).await.expect("save");
    let loaded = repo
        .find_lease_by_fqdn("_acme-challenge.example.com")
        .await
        .expect("find")
        .expect("present");
    assert!(loaded.is_active());
    repo.revoke_lease(lease.id).await.expect("revoke");
    let loaded = repo
        .find_lease_by_fqdn("_acme-challenge.example.com")
        .await
        .expect("find")
        .expect("present");
    assert!(!loaded.is_active());
}

#[tokio::test]
async fn non_admin_cannot_issue() {
    let db = TestDb::new().await;
    let repo = Arc::new(SqliteWildcardRepository::new(db.pool()));
    let dns = Arc::new(RecordingDnsProvider::new());
    let solver = Dns01ChallengeSolver::new(
        repo.clone(),
        dns.clone(),
        Arc::new(NoopAuditService),
    );
    let issuer = WildcardIssuer::new(repo, solver, Arc::new(NoopAuditService));
    let user = openpanel_domain::User::new(
        Uuid::new_v4(),
        openpanel_domain::Username::new("viewer").expect("static"),
        openpanel_domain::Email::new("viewer@example.com").expect("static"),
        openpanel_domain::Password::hash("correct horse battery staple").expect("static"),
        Role::User,
    );
    let res = issuer.issue(&user, make_request(true)).await;
    assert!(matches!(res, Err(openpanel_domain::WildcardError::Forbidden)));
}

#[tokio::test]
async fn default_endpoint_is_staging() {
    let request = make_request(true);
    assert_eq!(
        request.endpoint_mode.endpoint(),
        "https://acme-staging-v02.api.letsencrypt.org/directory"
    );
}
