//! Global FTP aggregate + database row-action HTTP integration tests.
//!
//! Capabilities under test: `ftp` (new global aggregate delta),
//! `databases` (row-action enrichment + adopt delta), and the
//! `capability-navigation` discoverability delta for `/ftp`: Owner
//! renders the aggregate with CSRF-free reads; unauthenticated
//! callers redirect; Users see only accessible sites; DB row
//! mutations are CSRF + permission gated with no plaintext in list
//! HTML; the phpMyAdmin deep-link rests on the existing admin-tool
//! single-use SSO mint (replay returns 401, one audit row).

use openpanel_app::{AdminToolSso, CreateFtpAccount, SqliteDbPrivilegeRepository};
use openpanel_core::audit::SqliteAuditService;
use openpanel_domain::Role;
use openpanel_web::capability_registry;
use uuid::Uuid;

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

fn csrf_token_from_html(html: &str) -> String {
    let marker = "name=\"_csrf\" value=\"";
    let start = html.find(marker).expect("csrf field in html") + marker.len();
    let end = html[start..].find('"').expect("csrf value end") + start;
    html[start..end].to_string()
}

async fn owner_user(server: &TestServer) -> openpanel_domain::User {
    server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.username().as_str() == "owner")
        .unwrap()
}

async fn make_site(server: &TestServer, domain: &str) -> uuid::Uuid {
    let user = owner_user(server).await;
    server
        .sites()
        .create_site(&user, user.id(), domain, vec![], false, None, None)
        .await
        .unwrap()
        .id()
}

async fn make_ftp_account(server: &TestServer, site_id: Uuid, username: &str) {
    let user = owner_user(server).await;
    server
        .ftp()
        .create(
            &user,
            site_id,
            CreateFtpAccount {
                username: username.to_string(),
                password: "correct horse battery staple".to_string(),
                read_only: false,
                bandwidth_kb_per_session: None,
                max_concurrent_connections: None,
            },
        )
        .await
        .unwrap();
}

/// GET /ftp as Owner renders the aggregate with site links and no
/// credential material; unauthenticated callers get 302 to /login.
#[tokio::test]
async fn ftp_global_list_renders() {
    let server = TestServer::new().await;
    let cookie = seed_owner(&server).await;
    let site_id = make_site(&server, "ftp-global.example.test").await;
    make_ftp_account(&server, site_id, "uploads").await;

    let resp = server
        .client()
        .get(format!("{}/ftp", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /ftp");
    assert_eq!(resp.status(), 200);
    let body = resp.text().await.expect("body");
    assert!(body.contains("<h1>FTP</h1>"), "page title: {body}");
    assert!(body.contains("uploads"), "account row: {body}");
    assert!(
        body.contains("ftp-global.example.test"),
        "site cell: {body}"
    );
    assert!(
        body.contains(&format!("/sites/{site_id}/ftp")),
        "site link: {body}"
    );
    assert!(
        !body.contains("correct horse battery staple"),
        "no credential material: {body}"
    );

    let anon = server
        .client()
        .get(format!("{}/ftp", server.base_url()))
        .send()
        .await
        .expect("anon GET /ftp");
    assert_eq!(anon.status(), 302, "unauthenticated redirects");
    let location = anon
        .headers()
        .get(reqwest::header::LOCATION)
        .expect("location")
        .to_str()
        .expect("location str");
    assert_eq!(location, "/login");
}

/// GET /ftp scoping: a User sees only rows for sites they own;
/// Owner sees all rows.
#[tokio::test]
async fn ftp_global_list_scoped() {
    let server = TestServer::new().await;
    let owner_cookie = seed_owner(&server).await;
    let owner_site = make_site(&server, "owner-site.example.test").await;
    make_ftp_account(&server, owner_site, "owner-uploads").await;

    server
        .identity()
        .create_user(
            "member",
            "member@example.test",
            "correct horse battery staple",
            Role::User,
            "test",
        )
        .await
        .expect("create user");
    let member = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.username().as_str() == "member")
        .unwrap();
    let owner = owner_user(&server).await;
    let member_site = server
        .sites()
        .create_site(
            &owner,
            member.id(),
            "member-site.example.test",
            vec![],
            false,
            None,
            None,
        )
        .await
        .unwrap()
        .id();
    server
        .ftp()
        .create(
            &owner,
            member_site,
            CreateFtpAccount {
                username: "member-uploads".to_string(),
                password: "correct horse battery staple".to_string(),
                read_only: false,
                bandwidth_kb_per_session: None,
                max_concurrent_connections: None,
            },
        )
        .await
        .unwrap();
    let member_cookie = login(&server, "member", "correct horse battery staple").await;

    let member_body = server
        .client()
        .get(format!("{}/ftp", server.base_url()))
        .header(reqwest::header::COOKIE, &member_cookie)
        .send()
        .await
        .expect("member GET /ftp")
        .text()
        .await
        .expect("body");
    assert!(
        member_body.contains("member-uploads"),
        "own row visible: {member_body}"
    );
    assert!(
        !member_body.contains("owner-uploads"),
        "other site hidden: {member_body}"
    );

    let owner_body = server
        .client()
        .get(format!("{}/ftp", server.base_url()))
        .header(reqwest::header::COOKIE, &owner_cookie)
        .send()
        .await
        .expect("owner GET /ftp")
        .text()
        .await
        .expect("body");
    assert!(
        owner_body.contains("member-uploads"),
        "owner sees member row: {owner_body}"
    );
    assert!(
        owner_body.contains("owner-uploads"),
        "owner sees own row: {owner_body}"
    );
}

/// DB row mutations (import, permission, tools, quota) require valid
/// CSRF and caller permission; bad CSRF gets 403 and list HTML never
/// contains plaintext.
#[tokio::test]
async fn db_row_actions_gated() {
    let server = TestServer::new().await;
    let cookie = seed_owner(&server).await;
    let missing = Uuid::new_v4();

    // List HTML carries no credential material.
    let list = server
        .client()
        .get(format!("{}/databases", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("GET /databases");
    assert_eq!(list.status(), 200);
    let list_body = list.text().await.expect("body");
    assert!(
        !list_body.contains("correct horse battery staple"),
        "no plaintext in list: {list_body}"
    );

    // Every row mutation with a bad token is refused before any work.
    for (method, url, form) in [
        (
            "post",
            format!("{}/databases/{missing}/import", server.base_url()),
            vec![("_csrf", "wrong"), ("filename", "dump.sql")],
        ),
        (
            "post",
            format!("{}/databases/{missing}/permission", server.base_url()),
            vec![
                ("_csrf", "wrong"),
                ("mode", "local"),
                ("cidrs", ""),
                ("tool", ""),
                ("quota_mb", ""),
                ("server", ""),
                ("name", ""),
                ("filename", ""),
            ],
        ),
        (
            "post",
            format!("{}/databases/{missing}/tools", server.base_url()),
            vec![("_csrf", "wrong"), ("tool", "repair")],
        ),
        (
            "post",
            format!("{}/databases/{missing}/quota", server.base_url()),
            vec![("_csrf", "wrong"), ("quota_mb", "1024")],
        ),
        (
            "post",
            format!("{}/databases/adopt", server.base_url()),
            vec![("_csrf", "wrong"), ("server", "a"), ("name", "b")],
        ),
    ] {
        let _ = method;
        let denied = server
            .client()
            .post(&url)
            .header(reqwest::header::COOKIE, &cookie)
            .form(&form)
            .send()
            .await
            .expect("bad csrf post");
        assert_eq!(denied.status(), 403, "bad csrf refused at {url}");
    }

    // Non-manager role is denied without leaking anything.
    server
        .identity()
        .create_user(
            "dbmember",
            "dbmember@example.test",
            "correct horse battery staple",
            Role::User,
            "test",
        )
        .await
        .expect("create user");
    let user_cookie = login(&server, "dbmember", "correct horse battery staple").await;
    let denied = server
        .client()
        .post(format!("{}/databases/{missing}/tools", server.base_url()))
        .header(reqwest::header::COOKIE, &user_cookie)
        .form(&[("_csrf", "whatever"), ("tool", "repair")])
        .send()
        .await
        .expect("user post tools");
    assert_eq!(denied.status(), 403, "user denied");

    // Extension gate works with a valid session (no database row
    // needed: validation precedes any service call).
    let page = server
        .client()
        .get(format!("{}/databases", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .send()
        .await
        .expect("csrf page")
        .text()
        .await
        .expect("body");
    let csrf = csrf_token_from_html(&page);
    let rejected = server
        .client()
        .post(format!("{}/databases/{missing}/import", server.base_url()))
        .header(reqwest::header::COOKIE, &cookie)
        .form(&[("_csrf", csrf.as_str()), ("filename", "backup.exe")])
        .send()
        .await
        .expect("bad extension post");
    assert_eq!(rejected.status(), 413, "bad extension rejected");
    let rejected_body = rejected.text().await.expect("body");
    assert!(
        rejected_body.contains("form-error"),
        "inline error: {rejected_body}"
    );
    assert!(
        !rejected_body.contains("correct horse battery staple"),
        "no plaintext on reject: {rejected_body}"
    );
}

/// The phpMyAdmin deep-link rests on the existing admin-tool
/// single-use SSO mint: issue consumes once, replay fails, and the
/// launch audits `AdminToolLaunched` exactly once.
#[tokio::test]
async fn db_phpmyadmin_sso_single_use() {
    let server = TestServer::new().await;
    server
        .bootstrap_owner("ssoowner", "correct horse battery staple")
        .await;
    let owner = server
        .identity()
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .find(|user| user.username().as_str() == "ssoowner")
        .unwrap();
    let pool = server.database_pool();
    let repo = std::sync::Arc::new(SqliteDbPrivilegeRepository::new(pool.clone()));
    let audit = std::sync::Arc::new(SqliteAuditService::new(pool.clone()));
    let sso = AdminToolSso::new(repo, audit);
    let db_id = Uuid::new_v4();

    let session = sso.issue(&owner, db_id, owner.id()).await.expect("mint");
    let consumed = sso
        .consume(&owner, session.id, &session.token)
        .await
        .expect("first consume ok");
    assert_eq!(consumed.id, session.id);
    let replay = sso.consume(&owner, session.id, &session.token).await;
    assert!(
        matches!(
            replay,
            Err(openpanel_domain::DbPrivilegeError::InvalidSsoToken)
        ),
        "replay refused: {replay:?}"
    );

    let launches = server
        .audit_events()
        .await
        .into_iter()
        .filter(|event| {
            event.action.as_str() == "db_admin_tool_launched"
                && event.target.as_deref() == Some(&db_id.to_string())
        })
        .collect::<Vec<_>>();
    assert_eq!(launches.len(), 1, "one launch audit: {launches:?}");
}

/// Registry/route agreement: `find_by_route("/ftp")` resolves to the
/// `ftp-global` entry and the static router-mount guard covers `/ftp`.
#[tokio::test]
async fn ftp_global_discoverable() {
    let entry = capability_registry::find_by_route("/ftp").expect("ftp entry");
    assert_eq!(entry.key, "ftp-global");
    assert_eq!(entry.capability, "ftp");
    assert_eq!(entry.label, "FTP");
    assert!(matches!(
        entry.scope,
        capability_registry::RouteScope::Global
    ));
    assert!(capability_registry::template_matches("/ftp", "/ftp"));
    assert!(capability_registry::role_may_see(Role::User, entry));
    assert!(capability_registry::role_may_see(Role::Owner, entry));

    const ROUTER: &str = include_str!("../../crates/openpanel-web/src/router.rs");
    assert!(
        ROUTER.contains("\"/ftp\""),
        "ftp route mounted in router.rs"
    );
}
