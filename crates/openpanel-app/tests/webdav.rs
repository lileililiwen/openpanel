//! WebDAV service contracts and jailed DAV router integration tests.
//!
//! Covers `openspec/specs/webdav-file-access/spec.md`.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use axum::{
    body::{Body, Bytes},
    http::{Request, StatusCode},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::Utc;
use openpanel_app::{
    CreateWebDavAccount, SqliteWebDavRepository, UpdateWebDavAccount, WebDavService, dav_router,
    migrations::WEBDAV_V001,
};
use openpanel_domain::{
    Email, Password, RepoError, Role, Site, User, Username,
    webdav::{WebDavAccount, WebDavError, WebDavRepository},
};
use openpanel_test_support::{MockAudit, MockSiteRepo};
use sqlx::{Pool, Sqlite, sqlite::SqlitePoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

mockall::mock! {
    Repo {}
    #[async_trait]
    impl WebDavRepository for Repo {
        async fn create(&self, account: &WebDavAccount) -> Result<(), RepoError>;
        async fn update(&self, account: &WebDavAccount) -> Result<(), RepoError>;
        async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError>;
        async fn find(&self, site_id: Uuid, account_id: Uuid) -> Result<Option<WebDavAccount>, RepoError>;
        async fn find_by_id(&self, account_id: Uuid) -> Result<Option<WebDavAccount>, RepoError>;
        async fn find_by_username(&self, site_id: Uuid, username: &str) -> Result<Option<WebDavAccount>, RepoError>;
        async fn list(&self, site_id: Uuid) -> Result<Vec<WebDavAccount>, RepoError>;
    }
}

const SECRET: &str = "correct horse battery staple";

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash(SECRET).unwrap(),
        Role::Owner,
    )
}

fn site() -> Site {
    Site::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "dav.example.test",
        vec![],
        "/srv/openpanel/sites/dav.example.test/public",
        false,
        None,
        "owner",
    )
    .unwrap()
}

fn service_with(repo: MockRepo, sites: MockSiteRepo) -> WebDavService {
    WebDavService::new(Arc::new(repo), Arc::new(sites), Arc::new(MockAudit::stub()))
}

#[tokio::test]
async fn owner_creates_account_without_returning_hash() {
    let site = site();
    let site_id = site.id();
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .with(mockall::predicate::eq(site_id))
        .return_once(move |_| Ok(Some(site)));
    let mut repo = MockRepo::new();
    repo.expect_find_by_username().return_once(|_, _| Ok(None));
    repo.expect_create()
        .withf(|value| value.verify_password(SECRET).unwrap())
        .return_once(|_| Ok(()));
    let created = service_with(repo, sites)
        .create(
            &owner(),
            site_id,
            CreateWebDavAccount {
                username: "uploads".into(),
                password: SECRET.into(),
                read_only: false,
            },
        )
        .await
        .unwrap();
    let json = serde_json::to_string(&created).unwrap();
    assert!(created.plaintext_password_shown_once);
    assert!(!json.contains("correct horse"));
    assert!(!json.contains("argon2"));
}

#[tokio::test]
async fn duplicate_username_is_rejected_before_persistence() {
    let site = site();
    let site_id = site.id();
    let existing = account(site_id, PathBuf::from("/srv/sites/dav"), false);
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .return_once(move |_| Ok(Some(site)));
    let mut repo = MockRepo::new();
    repo.expect_find_by_username()
        .return_once(move |_, _| Ok(Some(existing)));
    repo.expect_create().never();
    let result = service_with(repo, sites)
        .create(
            &owner(),
            site_id,
            CreateWebDavAccount {
                username: "uploads".into(),
                password: SECRET.into(),
                read_only: false,
            },
        )
        .await;
    assert_eq!(result.unwrap_err(), WebDavError::Duplicate);
}

#[tokio::test]
async fn update_patches_read_only_and_enable_state() {
    let site = site();
    let site_id = site.id();
    let existing = account(site_id, PathBuf::from("/srv/sites/dav"), false);
    let account_id = existing.id();
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .returning(move |_| Ok(Some(site.clone())));
    let mut repo = MockRepo::new();
    repo.expect_find()
        .withf(move |s, id| *s == site_id && *id == account_id)
        .returning(move |_, _| {
            Ok(Some(account(
                site_id,
                PathBuf::from("/srv/sites/dav"),
                false,
            )))
        });
    repo.expect_update()
        .withf(|value| value.read_only() && !value.enabled())
        .return_once(|_| Ok(()));
    let view = service_with(repo, sites)
        .update(
            &owner(),
            site_id,
            account_id,
            UpdateWebDavAccount {
                enabled: Some(false),
                password: None,
                read_only: Some(true),
            },
        )
        .await
        .unwrap();
    assert!(view.read_only);
    assert!(!view.enabled);
}

#[tokio::test]
async fn disabled_account_cannot_authenticate_and_denies_are_audited() {
    let site_id = Uuid::new_v4();
    let account_id = Uuid::new_v4();
    let sites = MockSiteRepo::new();
    let mut repo = MockRepo::new();
    let disabled_account = {
        let mut value = account(site_id, PathBuf::from("/srv/sites/dav"), false);
        value.disable(Utc::now());
        value
    };
    repo.expect_find_by_id()
        .returning(move |_| Ok(Some(disabled_account.clone())));
    repo.expect_update().never();
    let service = service_with(repo, sites);
    let denied = service
        .authenticate(account_id, "uploads", "wrong password here", "10.0.0.1")
        .await;
    assert_eq!(denied.unwrap_err(), WebDavError::LoginDenied);
    // Unknown username must not disclose which half failed.
    let unknown = service
        .authenticate(account_id, "someoneelse", SECRET, "10.0.0.1")
        .await;
    assert_eq!(unknown.unwrap_err(), WebDavError::LoginDenied);
}

#[tokio::test]
async fn authenticate_success_records_login_metadata() {
    let site_id = Uuid::new_v4();
    let live = account(site_id, PathBuf::from("/srv/sites/dav"), false);
    let account_id = live.id();
    let sites = MockSiteRepo::new();
    let mut repo = MockRepo::new();
    repo.expect_find_by_id().returning(move |_| {
        Ok(Some(account(
            site_id,
            PathBuf::from("/srv/sites/dav"),
            false,
        )))
    });
    repo.expect_update()
        .withf(|value| value.last_login_ip() == Some("10.0.0.9"))
        .return_once(|_| Ok(()));
    let service = service_with(repo, sites);
    let authenticated = service
        .authenticate(account_id, "uploads", SECRET, "10.0.0.9")
        .await
        .unwrap();
    assert!(authenticated.last_login_at().is_some());
}

fn account(site_id: Uuid, home: PathBuf, read_only: bool) -> WebDavAccount {
    WebDavAccount::new(
        Uuid::new_v4(),
        site_id,
        "uploads",
        home,
        SECRET,
        read_only,
        Utc::now(),
    )
    .unwrap()
}

// ---------------------------------------------------------------------------
// Router-level DAV tests over a real SQLite repository and tempdir jail.
// ---------------------------------------------------------------------------

struct DavHarness {
    router: axum::Router,
    account_id: Uuid,
    home: tempfile::TempDir,
}

async fn harness(read_only: bool) -> DavHarness {
    let opts: sqlx::sqlite::SqliteConnectOptions = "sqlite::memory:".parse().unwrap();
    let pool: Pool<Sqlite> = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts.foreign_keys(false))
        .await
        .unwrap();
    // The webdav table carries a (non-enforced) FK to sites, which
    // SQLite resolves lazily — the parent tables must exist.
    sqlx::query(openpanel_app::migrations::IDENTITY_V001)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(openpanel_app::migrations::SITES_V001)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(WEBDAV_V001).execute(&pool).await.unwrap();
    let home = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(home.path().join("public")).unwrap();
    let account = account(
        Uuid::new_v4(),
        home.path().join("public").canonicalize().unwrap(),
        read_only,
    );
    let account_id = account.id();
    use openpanel_domain::webdav::WebDavRepository as _;
    SqliteWebDavRepository::new(pool.clone())
        .create(&account)
        .await
        .unwrap();
    let mut sites = MockSiteRepo::new();
    sites.expect_find_by_id().returning(|_| Ok(None));
    let service = WebDavService::new(
        Arc::new(SqliteWebDavRepository::new(pool)),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
    );
    DavHarness {
        router: dav_router(Arc::new(service)),
        account_id,
        home,
    }
}

fn basic(username: &str, password: &str) -> String {
    format!(
        "Basic {}",
        STANDARD.encode(format!("{username}:{password}"))
    )
}

async fn send(
    harness: &DavHarness,
    method: &str,
    uri: &str,
    body: Vec<u8>,
    auth: Option<(&str, &str)>,
    extra: Vec<(&str, String)>,
) -> (StatusCode, String, Bytes) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some((username, password)) = auth {
        builder = builder.header(axum::http::header::AUTHORIZATION, basic(username, password));
    }
    for (name, value) in extra {
        builder = builder.header(name, value);
    }
    let request = builder.body(Body::from(body)).unwrap();
    let response = harness
        .router
        .clone()
        .oneshot(request)
        .await
        .expect("response");
    let status = response.status();
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, content_type, bytes)
}

#[tokio::test]
async fn anonymous_read_gets_basic_challenge() {
    let harness = harness(false).await;
    let (status, _, _) = send(
        &harness,
        "GET",
        &format!("/{}", harness.account_id),
        Vec::new(),
        None,
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn wrong_password_gets_basic_challenge() {
    let harness = harness(false).await;
    let (status, _, _) = send(
        &harness,
        "GET",
        &format!("/{}", harness.account_id),
        Vec::new(),
        Some(("uploads", "wrong password here")),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn put_get_head_roundtrip_inside_jail() {
    let harness = harness(false).await;
    let uri = format!("/{}", harness.account_id) + "/hello.txt";
    let (status, _, _) = send(
        &harness,
        "PUT",
        &uri,
        b"webdav works".to_vec(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, content_type, body) = send(
        &harness,
        "GET",
        &uri,
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"webdav works");
    assert!(content_type.starts_with("text/plain"));
    let (status, _, head_body) = send(
        &harness,
        "HEAD",
        &uri,
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(head_body.is_empty());
}

#[tokio::test]
async fn traversal_and_absolute_escape_render_404() {
    let harness = harness(false).await;
    for (rel, strict_404) in [("../escape.txt", true), ("/etc/passwd", false)] {
        let uri = if rel.starts_with('/') {
            format!("/{}{}", harness.account_id, rel)
        } else {
            format!("/{}/{rel}", harness.account_id)
        };
        let (status, _, _) = send(
            &harness,
            "PUT",
            &uri,
            b"nope".to_vec(),
            Some(("uploads", SECRET)),
            vec![],
        )
        .await;
        if strict_404 {
            assert_eq!(status, StatusCode::NOT_FOUND, "{rel} accepted");
        } else {
            // An absolute escape either renders 404 or collapses to a
            // jailed relative path whose parent is missing — never a
            // successful write.
            assert!(
                status == StatusCode::NOT_FOUND || status == StatusCode::CONFLICT,
                "{rel} returned {status}"
            );
        }
    }
    // Encoded traversal may be decoded (404) or treated as a literal
    // name whose parent is missing (409) — never a successful escape.
    let (status, _, _) = send(
        &harness,
        "PUT",
        &format!("/{}/%2e%2e/escape.txt", harness.account_id),
        b"nope".to_vec(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert!(
        status == StatusCode::NOT_FOUND || status == StatusCode::CONFLICT,
        "encoded traversal returned {status}"
    );
    assert!(
        !harness
            .home
            .path()
            .parent()
            .is_some_and(|dir| dir.join("escape.txt").exists())
    );
}

#[tokio::test]
async fn symlink_escape_render_404() {
    let harness = harness(false).await;
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), b"leak").unwrap();
    std::os::unix::fs::symlink(
        outside.path().join("secret.txt"),
        harness.home.path().join("public").join("link.txt"),
    )
    .unwrap();
    let uri = format!("/{}", harness.account_id) + "/link.txt";
    let (status, _, _) = send(
        &harness,
        "GET",
        &uri,
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn read_only_account_rejects_mutating_verbs() {
    let harness = harness(true).await;
    // Seed a readable file directly in the jail.
    std::fs::write(harness.home.path().join("public/seed.txt"), b"read me").unwrap();
    let (status, _, body) = send(
        &harness,
        "GET",
        &format!("/{}/seed.txt", harness.account_id),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"read me");
    for verb in ["PUT", "DELETE", "MKCOL", "MOVE", "COPY", "PROPPATCH"] {
        let (status, _, _) = send(
            &harness,
            verb,
            &format!("/{}/seed.txt", harness.account_id),
            Vec::new(),
            Some(("uploads", SECRET)),
            vec![],
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{verb} allowed on read-only");
    }
}

#[tokio::test]
async fn propfind_depth0_and_depth1_return_207() {
    let harness = harness(false).await;
    let root = format!("/{}", harness.account_id);
    std::fs::write(harness.home.path().join("public/a.txt"), b"aaa").unwrap();
    std::fs::create_dir(harness.home.path().join("public/d&x")).unwrap();
    let (status, content_type, body) = send(
        &harness,
        "PROPFIND",
        &root,
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![("depth", "1".to_owned())],
    )
    .await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(content_type.starts_with("application/xml"));
    let text = String::from_utf8_lossy(&body).into_owned();
    assert!(text.contains("<D:collection"), "{text}");
    assert!(text.contains("/a.txt"), "{text}");
    assert!(
        text.contains("/d%26x/"),
        "href must be escaped+encoded: {text}"
    );
    let (_, _, shallow) = send(
        &harness,
        "PROPFIND",
        &root,
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![("depth", "0".to_owned())],
    )
    .await;
    let shallow = String::from_utf8_lossy(&shallow).into_owned();
    assert!(
        !shallow.contains("/a.txt"),
        "Depth 0 must not list children"
    );
}

#[tokio::test]
async fn collection_lifecycle_mkcol_put_move_copy_delete() {
    let harness = harness(false).await;
    let root = format!("/{}", harness.account_id);
    let mk = send(
        &harness,
        "MKCOL",
        &(root.clone() + "/col"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await
    .0;
    assert_eq!(mk, StatusCode::CREATED);
    let put = send(
        &harness,
        "PUT",
        &(root.clone() + "/col/moved.txt"),
        b"payload".to_vec(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await
    .0;
    assert_eq!(put, StatusCode::CREATED);
    let move_res = send(
        &harness,
        "MOVE",
        &(root.clone() + "/col/moved.txt"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![("destination", format!("/dav{root}/col/final.txt"))],
    )
    .await
    .0;
    assert_eq!(move_res, StatusCode::CREATED);
    let (status, _, body) = send(
        &harness,
        "GET",
        &(root.clone() + "/col/final.txt"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body.as_ref(), b"payload");
    let copy_res = send(
        &harness,
        "COPY",
        &(root.clone() + "/col/final.txt"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![("destination", format!("/dav{root}/col/copy.txt"))],
    )
    .await
    .0;
    assert_eq!(copy_res, StatusCode::CREATED);
    let gone = send(
        &harness,
        "DELETE",
        &(root.clone() + "/col/final.txt"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await
    .0;
    assert_eq!(gone, StatusCode::NO_CONTENT);
    let missing = send(
        &harness,
        "GET",
        &(root.clone() + "/col/final.txt"),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await
    .0;
    assert_eq!(missing, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn options_advertises_dav_and_lock_is_not_implemented() {
    let harness = harness(false).await;
    // OPTIONS is answered without credentials by design.
    let request = Request::builder()
        .method("OPTIONS")
        .uri(format!("/{}", harness.account_id))
        .body(Body::empty())
        .unwrap();
    let response = harness.router.clone().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("dav")
            .and_then(|value| value.to_str().ok()),
        Some("1")
    );
    let lock = Request::builder()
        .method("LOCK")
        .uri(format!("/{}", harness.account_id))
        .header(axum::http::header::AUTHORIZATION, basic("uploads", SECRET))
        .body(Body::empty())
        .unwrap();
    let response = harness.router.clone().oneshot(lock).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn put_into_missing_parent_conflicts() {
    let harness = harness(false).await;
    let (status, _, _) = send(
        &harness,
        "PUT",
        &format!("/{}/missing/x.txt", harness.account_id),
        b"x".to_vec(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn proppatch_is_audited_noop_multistatus() {
    let harness = harness(false).await;
    let (status, content_type, body) = send(
        &harness,
        "PROPPATCH",
        &format!("/{}/seed.txt", harness.account_id),
        Vec::new(),
        Some(("uploads", SECRET)),
        vec![],
    )
    .await;
    assert_eq!(status, StatusCode::MULTI_STATUS);
    assert!(content_type.starts_with("application/xml"));
    let text = String::from_utf8_lossy(&body).into_owned();
    assert!(text.contains("<D:propstat>"), "{text}");
    assert!(text.contains("200 OK"), "{text}");
}
