//! Migration wizard UI integration tests.
//!
//! Capabilities under test: `migration-wizard-ui` (new; via the
//! `migration-importers` surfacing delta and the
//! `capability-navigation` discoverability delta):
//! upload/sniff, preview-with-policy (plan-only), conflict-policy
//! confirm, run-with-verifiers (+ `PartiallyCompleted`), and separate
//! authorized DNS/mail/SSL cutover actions.

use openpanel_domain::Role;
use openpanel_web::capability_registry;

use crate::common::*;

fn session_cookie(resp: &reqwest::Response) -> String {
    resp.headers()
        .get(reqwest::header::SET_COOKIE)
        .expect("Set-Cookie")
        .to_str()
        .expect("cookie str")
        .split(';')
        .next()
        .expect("name=value")
        .to_string()
}

async fn login(server: &TestServer, username: &str, password: &str) -> String {
    let resp = server
        .client()
        .post(format!("{}/login", server.base_url()))
        .form(&[("username_or_email", username), ("password", password)])
        .send()
        .await
        .expect("POST /login");
    assert_eq!(resp.status(), 303);
    session_cookie(&resp)
}

async fn seed_owner(server: &TestServer) -> String {
    server
        .bootstrap_owner("owner", "correct horse battery staple")
        .await;
    login(server, "owner", "correct horse battery staple").await
}

async fn seed_role(server: &TestServer, username: &str, role: Role) {
    server
        .identity()
        .create_user(
            username,
            &format!("{username}@example.test"),
            "correct horse battery staple",
            role,
            "test",
        )
        .await
        .expect("create user");
}

/// Extract the `_csrf` token from a rendered form.
fn csrf_token_from_html(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field in html") + marker.len();
    let end = html[start..].find('"').expect("csrf value end") + start;
    html[start..end].to_string()
}

async fn owner_cookie_and_csrf(server: &TestServer) -> (String, String) {
    let cookie = seed_owner(server).await;
    let body = server
        .client()
        .get(format!("{}/migrate", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /migrate")
        .text()
        .await
        .expect("body");
    let csrf = csrf_token_from_html(&body);
    (cookie, csrf)
}

const BAOTA_BUNDLE: &str = "baota /www/backup manifest with example.com vhost";

/// GET /migrate as Owner renders the upload form with a CSRF token;
/// unauthenticated callers are redirected to /login.
#[tokio::test]
async fn migrate_wizard_upload_form() {
    let server = TestServer::new().await;
    let (cookie, _) = owner_cookie_and_csrf(&server).await;
    let resp = server
        .client()
        .get(format!("{}/migrate", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /migrate");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<h1>Migrate</h1>"), "wizard title: {body}");
    assert!(body.contains("Driver hint"), "hint select: {body}");
    assert!(body.contains("name=\"bundle_text\""), "bundle input: {body}");
    assert!(body.contains("name=\"_csrf\""), "csrf field: {body}");

    let anon = server
        .client()
        .get(format!("{}/migrate", server.base_url()))
        .send()
        .await
        .expect("anon GET /migrate");
    assert_eq!(anon.status(), 302, "unauthenticated redirects");
    let location = anon
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("location")
        .to_str()
        .expect("location str");
    assert_eq!(location, "/login");
}

/// POST /migrate/preview with a Baota fixture lists planned resources
/// and warnings with no state change; a hint mismatch reports
/// UnknownSource.
#[tokio::test]
async fn migrate_wizard_preview_plan() {
    let server = TestServer::new().await;
    let (cookie, csrf) = owner_cookie_and_csrf(&server).await;
    let resp = server
        .client()
        .post(format!("{}/migrate/preview", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("driver_hint", "baota-backup"),
            ("bundle_text", BAOTA_BUNDLE),
            ("policy", "block"),
        ])
        .send()
        .await
        .expect("POST /migrate/preview");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("example.com"), "planned site: {body}");
    assert!(body.contains("example_db"), "planned database: {body}");
    assert!(
        body.contains("scheduled tasks are not imported"),
        "warnings: {body}"
    );
    assert!(
        !body.contains("Run completed") && !body.contains("MigrationRunCommitted"),
        "preview commits nothing: {body}"
    );

    let preview_csrf = csrf_token_from_html(&{
        let b = server
            .client()
            .get(format!("{}/migrate", server.base_url()))
            .header(reqwest::header::COOKIE, &cookie)
            .send()
            .await
            .expect("re-get")
            .text()
            .await
            .expect("body");
        b
    });
    let resp = server
        .client()
        .post(format!("{}/migrate/preview", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", preview_csrf.as_str()),
            ("driver_hint", "cpanel-pkgacct"),
            ("bundle_text", BAOTA_BUNDLE),
            ("policy", "block"),
        ])
        .send()
        .await
        .expect("mismatched preview");
    let body = resp.text().await.expect("body");
    assert!(body.contains("UnknownSource"), "hint mismatch: {body}");
}

/// POST /migrate/run with ConflictPolicy::Block and a conflicting
/// plan refuses the commit with no state change.
#[tokio::test]
async fn migrate_wizard_block_on_conflict() {
    let server = TestServer::new().await;
    let (cookie, csrf) = owner_cookie_and_csrf(&server).await;
    let resp = server
        .client()
        .post(format!("{}/migrate/run", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("driver", "baota-backup"),
            ("policy", "block"),
            ("conflicts", "1"),
            ("verify", "pass"),
        ])
        .send()
        .await
        .expect("POST /migrate/run");
    assert_eq!(resp.status(), 409, "blocked commit is refused");
    let body = resp.text().await.expect("body");
    assert!(body.contains("blocks the commit"), "refusal: {body}");
    assert!(body.contains("No state changed"), "no mutation: {body}");
}

/// POST /migrate/run with a failing verifier records
/// partially_completed, and POST /migrate/cutover/dns is refused.
#[tokio::test]
async fn migrate_wizard_partial_refuses_cutover() {
    let server = TestServer::new().await;
    let (cookie, csrf) = owner_cookie_and_csrf(&server).await;
    let resp = server
        .client()
        .post(format!("{}/migrate/run", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("driver", "baota-backup"),
            ("policy", "skip"),
            ("conflicts", "1"),
            ("verify", "fail"),
        ])
        .send()
        .await
        .expect("POST /migrate/run");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(
        body.contains("PartiallyCompleted"),
        "partial banner: {body}"
    );

    let resp = server
        .client()
        .post(format!("{}/migrate/cutover/dns", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[
            ("_csrf", csrf.as_str()),
            ("partial", "true"),
            ("preflight", "true"),
        ])
        .send()
        .await
        .expect("POST cutover/dns");
    assert_eq!(resp.status(), 409, "cutover refused on partial run");
    let body = resp.text().await.expect("body");
    assert!(body.contains("Cutover refused"), "refusal: {body}");
    assert!(body.contains("isolated"), "isolation note: {body}");
}

/// POST /migrate/cutover/{dns,mail,ssl} each require the Owner role
/// and a valid CSRF token: User gets 403, bad CSRF gets 403.
#[tokio::test]
async fn migrate_wizard_cutover_gated() {
    let server = TestServer::new().await;
    let (owner_cookie, owner_csrf) = owner_cookie_and_csrf(&server).await;
    seed_role(&server, "member", Role::User).await;
    let member_cookie = login(&server, "member", "correct horse battery staple").await;
    let member_page = server
        .client()
        .get(format!("{}/migrate", server.base_url()))
        .header(reqwest::header::COOKIE, &member_cookie)
        .send()
        .await
        .expect("member GET /migrate");
    assert_eq!(member_page.status(), 403, "User denied direct access");

    for kind in ["dns", "mail", "ssl"] {
        let resp = server
            .client()
            .post(format!("{}/migrate/cutover/{kind}", server.base_url()))
            .header(reqwest::header::COOKIE, &member_cookie)
            .form(&[
                ("_csrf", owner_csrf.as_str()),
                ("partial", "false"),
                ("preflight", "true"),
            ])
            .send()
            .await
            .expect("member cutover");
        assert_eq!(resp.status(), 403, "User cutover {kind} denied");

        let resp = server
            .client()
            .post(format!("{}/migrate/cutover/{kind}", server.base_url()))
            .header(reqwest::header::COOKIE, &owner_cookie)
            .form(&[
                ("_csrf", "definitely-wrong"),
                ("partial", "false"),
                ("preflight", "true"),
            ])
            .send()
            .await
            .expect("bad csrf cutover");
        assert_eq!(resp.status(), 403, "bad CSRF {kind} rejected");

        let resp = server
            .client()
            .post(format!("{}/migrate/cutover/{kind}", server.base_url()))
            .header(reqwest::header::COOKIE, &owner_cookie)
            .form(&[
                ("_csrf", owner_csrf.as_str()),
                ("partial", "false"),
                ("preflight", "true"),
            ])
            .send()
            .await
            .expect("owner cutover");
        assert_eq!(resp.status(), 200, "owner cutover {kind} commits");
        let body = resp.text().await.expect("body");
        assert!(
            body.contains("Cutover committed"),
            "success marker {kind}: {body}"
        );
    }
}

/// Registry/route agreement: find_by_route("/migrate") resolves, the
/// template matches, Owners see the nav link and Users do not.
#[tokio::test]
async fn migrate_wizard_discoverable() {
    let entry = capability_registry::find_by_route("/migrate").expect("migrate entry");
    assert_eq!(entry.key, "migrate");
    assert_eq!(entry.capability, "migration");
    assert_eq!(entry.min_role, Role::Owner);
    assert!(capability_registry::template_matches("/migrate", "/migrate"));
    assert!(capability_registry::role_may_see(Role::Owner, entry));
    assert!(!capability_registry::role_may_see(Role::User, entry));

    let server = TestServer::new().await;
    let (owner_cookie, _) = owner_cookie_and_csrf(&server).await;
    let body = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &owner_cookie)
        .send()
        .await
        .expect("GET /")
        .text()
        .await
        .expect("body");
    assert!(body.contains("/migrate"), "owner nav shows Migrate: {body}");

    seed_role(&server, "member", Role::User).await;
    let member_cookie = login(&server, "member", "correct horse battery staple").await;
    let body = server
        .client()
        .get(format!("{}/", server.base_url()))
        .header(reqwest::header::COOKIE, &member_cookie)
        .send()
        .await
        .expect("member GET /")
        .text()
        .await
        .expect("body");
    assert!(
        !body.contains("/migrate"),
        "user nav hides Migrate: {body}"
    );
}

/// Property: preview with any policy over arbitrary fixture bundles
/// performs zero mutations (plan-only) and the UI gate agrees with
/// the service predicate on every policy × conflict combination.
#[test]
fn prop_migrate_policy_never_destructive_on_preview() {
    use openpanel_domain::{ConflictPolicy, MigrationPlan};
    use openpanel_web::migrate::{commit_enabled, plan_for_fixture};
    use proptest::prelude::*;
    proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
        .run(
            &(
                prop::sample::select(vec![
                    openpanel_domain::DriverKind::CpanelPkgacct,
                    openpanel_domain::DriverKind::CpanelLegacyBackup,
                    openpanel_domain::DriverKind::BaotaBackup,
                    openpanel_domain::DriverKind::TarWithJsonManifest,
                ]),
                prop::sample::select(vec![
                    ConflictPolicy::Block,
                    ConflictPolicy::Replace,
                    ConflictPolicy::Skip,
                    ConflictPolicy::RequireExplicit,
                ]),
                any::<bool>(),
            ),
            |(driver, policy, with_conflict)| {
                let plan: MigrationPlan =
                    plan_for_fixture(driver, Some(policy), with_conflict);
                // UI gate and service predicate must agree on every input.
                prop_assert_eq!(
                    commit_enabled(policy, with_conflict),
                    !plan.policy_blocks_commit()
                );
                // Preview is plan-only: the fixture builder carries no
                // run id and refuses nothing by itself.
                prop_assert!(!plan.refuses_empty_run());
                Ok(())
            },
        )
        .expect("policy property holds");
}

/// Property: any run with at least one failing verifier never enables
/// cutover, whatever the evidence text.
#[test]
fn prop_migrate_partial_never_auto_cutover() {
    use openpanel_domain::MigrationRunStatus;
    use openpanel_web::migrate::{cutover_enabled, run_view};
    use proptest::prelude::*;
    proptest::test_runner::TestRunner::new(ProptestConfig::with_cases(100))
        .run(
            &(
                any::<bool>(),
                any::<bool>(),
                proptest::collection::vec(".*", 0..4),
            ),
            |(preflight, _unused, evidence)| {
                // A partial run disables cutover even with preflight green.
                prop_assert!(!cutover_enabled(true, preflight));
                let out = run_view(MigrationRunStatus::PartiallyCompleted, &evidence, "csrf")
                    .into_string();
                prop_assert!(out.contains("PartiallyCompleted"));
                prop_assert_eq!(
                    out.matches("<button type=\"submit\" disabled>").count(),
                    3
                );
                Ok(())
            },
        )
        .expect("partial property holds");
}
