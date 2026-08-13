//! FTP service contracts with mocked persistence and site ownership.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use std::{
    net::{IpAddr, Ipv4Addr},
    path::PathBuf,
    sync::Arc,
};

use async_trait::async_trait;
use chrono::Utc;
use openpanel_app::{CreateFtpAccount, FtpAuthenticator, FtpService, FtpSessionRegistry};
use openpanel_domain::{
    Email, Password, RepoError, Role, Site, SiteRepository, User, Username,
    ftp::{FtpAccount, FtpError, FtpLimits, FtpRepository, FtpTlsMode},
};
use openpanel_test_support::{MockAudit, MockSiteRepo};
use uuid::Uuid;

mockall::mock! {
    Repo {}
    #[async_trait]
    impl FtpRepository for Repo {
        async fn create(&self, account: &FtpAccount) -> Result<(), RepoError>;
        async fn update(&self, account: &FtpAccount) -> Result<(), RepoError>;
        async fn delete(&self, site_id: Uuid, account_id: Uuid) -> Result<bool, RepoError>;
        async fn find(&self, site_id: Uuid, account_id: Uuid) -> Result<Option<FtpAccount>, RepoError>;
        async fn find_by_username(&self, site_id: Uuid, username: &str) -> Result<Option<FtpAccount>, RepoError>;
        async fn list(&self, site_id: Uuid) -> Result<Vec<FtpAccount>, RepoError>;
    }
}

fn owner() -> User {
    User::new(
        Uuid::new_v4(),
        Username::new("owner").unwrap(),
        Email::new("owner@example.test").unwrap(),
        Password::hash("correct horse battery staple").unwrap(),
        Role::Owner,
    )
}

fn site() -> Site {
    Site::new(
        Uuid::new_v4(),
        Uuid::new_v4(),
        "ftp.example.test",
        vec![],
        "/srv/openpanel/sites/ftp.example.test/public",
        false,
        None,
        "owner",
    )
    .unwrap()
}

fn account(site_id: Uuid, max: u16) -> FtpAccount {
    FtpAccount::new(
        Uuid::new_v4(),
        site_id,
        "uploads",
        PathBuf::from("/srv/openpanel/sites/ftp.example.test/public"),
        "correct horse battery staple",
        false,
        FtpLimits::new(1024, max).unwrap(),
        Utc::now(),
    )
    .unwrap()
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
        .withf(|value| {
            value
                .verify_password("correct horse battery staple")
                .unwrap()
        })
        .return_once(|_| Ok(()));
    let service = FtpService::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(FtpSessionRegistry::default()),
    );
    let created = service
        .create(
            &owner(),
            site_id,
            CreateFtpAccount {
                username: "uploads".into(),
                password: "correct horse battery staple".into(),
                read_only: false,
                bandwidth_kb_per_session: None,
                max_concurrent_connections: None,
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
    let existing = account(site_id, 4);
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .return_once(move |_| Ok(Some(site)));
    let mut repo = MockRepo::new();
    repo.expect_find_by_username()
        .return_once(move |_, _| Ok(Some(existing)));
    repo.expect_create().never();
    let service = FtpService::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(FtpSessionRegistry::default()),
    );
    let result = service
        .create(
            &owner(),
            site_id,
            CreateFtpAccount {
                username: "uploads".into(),
                password: "correct horse battery staple".into(),
                read_only: false,
                bandwidth_kb_per_session: None,
                max_concurrent_connections: None,
            },
        )
        .await;
    assert_eq!(result.unwrap_err(), FtpError::Duplicate);
}

#[tokio::test]
async fn disabled_login_and_fifth_concurrent_session_are_denied() {
    let site = site();
    let site_id = site.id();
    let mut disabled = account(site_id, 4);
    disabled.disable(Utc::now());
    let mut sites = MockSiteRepo::new();
    sites
        .expect_find_by_id()
        .times(1)
        .return_once(move |_| Ok(Some(site)));
    let mut repo = MockRepo::new();
    repo.expect_find_by_username()
        .times(1)
        .return_once(move |_, _| Ok(Some(disabled)));
    let auth = FtpAuthenticator::new(
        Arc::new(repo),
        Arc::new(sites),
        Arc::new(MockAudit::stub()),
        Arc::new(FtpSessionRegistry::default()),
    );
    assert!(matches!(
        auth.authenticate(
            site_id,
            "uploads",
            "correct horse battery staple",
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        )
        .await,
        Err(FtpError::LoginDenied)
    ));

    let registry = FtpSessionRegistry::default();
    let limited = account(site_id, 4);
    for octet in 1..=4 {
        registry
            .open(&limited, IpAddr::V4(Ipv4Addr::new(127, 0, 0, octet)))
            .unwrap();
    }
    assert_eq!(registry.total(), 4);
    assert_eq!(
        registry
            .open(&limited, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 5)))
            .unwrap_err(),
        FtpError::ConnectionLimit
    );
}

#[test]
fn chroot_storage_denies_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret"), b"secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside.path().join("secret"), root.path().join("escape")).unwrap();
    let storage = openpanel_app::ChrootStorage::new(root.path()).unwrap();
    let path = openpanel_domain::ftp::FtpPath::new("escape").unwrap();
    assert_eq!(
        storage.resolve_existing(&path).unwrap_err(),
        FtpError::OperationDenied
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn live_libunftp_starttls_upload_download_and_chroot() {
    use openpanel_app::{
        FtpServerConfig, FtpServerTask, SqliteFtpRepository, sites::repo::SqliteSiteRepository,
    };
    use openpanel_core::{JobSupervisor, SqliteAuditService};
    use openpanel_test_support::TestDb;

    let db = TestDb::new().await;
    let pool = db.pool();
    for sql in [
        openpanel_app::migrations::IDENTITY_V001,
        openpanel_app::migrations::SITES_V001,
        openpanel_app::migrations::FTP_V001,
    ] {
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();
    }
    let owner_id = Uuid::new_v4();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO users (id,username,email,password_hash,role,created_at) VALUES (?,?,?,?,?,?)",
    )
    .bind(owner_id.to_string())
    .bind("ftpowner")
    .bind("ftpowner@example.test")
    .bind("unused")
    .bind("owner")
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let site = Site::new(
        Uuid::new_v4(),
        owner_id,
        "live-ftp.example.test",
        vec![],
        root.path().to_string_lossy(),
        false,
        None,
        "test",
    )
    .unwrap();
    let sites: Arc<dyn SiteRepository> = Arc::new(SqliteSiteRepository::new(pool.clone()));
    sites.insert(&site).await.unwrap();
    let repo: Arc<dyn FtpRepository> = Arc::new(SqliteFtpRepository::new(pool.clone()));
    let ftp_account = account(site.id(), 4);
    let ftp_account = FtpAccount::restore(
        ftp_account.id(),
        site.id(),
        "uploads".into(),
        root.path().to_path_buf(),
        ftp_account.password_hash().into(),
        false,
        FtpLimits::new(1, 4).unwrap(),
        true,
        None,
        None,
        Utc::now(),
        None,
    )
    .unwrap();
    repo.create(&ftp_account).await.unwrap();
    let sessions = Arc::new(FtpSessionRegistry::default());
    let audit = Arc::new(SqliteAuditService::new(pool));
    let auth = Arc::new(FtpAuthenticator::new(
        repo.clone(),
        sites,
        audit.clone(),
        sessions.clone(),
    ));
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
    let cert_path = root.path().join("cert.pem");
    let key_path = root.path().join("key.pem");
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind = listener.local_addr().unwrap();
    drop(listener);
    let task = FtpServerTask::new(
        FtpServerConfig {
            enabled: true,
            bind,
            tls_mode: FtpTlsMode::StartTls,
            cert_path: Some(cert_path),
            key_path: Some(key_path),
            passive_ports: 50110..=50120,
            global_max_connections: 32,
        },
        repo,
        auth,
        sessions,
        audit,
    );
    let supervisor = JobSupervisor::new().spawn(vec![Box::new(task)]);
    for _ in 0..40 {
        if tokio::net::TcpStream::connect(bind).await.is_ok() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let source = root.path().join("source.txt");
    let downloaded = root.path().join("downloaded.txt");
    std::fs::write(&source, b"openpanel ftp roundtrip").unwrap();
    let login = format!("{}@uploads:correct horse battery staple", site.id());
    let url = format!("ftp://{bind}/roundtrip.txt");
    let upload = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--ssl-reqd",
            "--insecure",
            "--user",
            &login,
            "--upload-file",
            source.to_string_lossy().as_ref(),
            &url,
        ])
        .output()
        .unwrap();
    assert!(
        upload.status.success(),
        "{}",
        String::from_utf8_lossy(&upload.stderr)
    );
    let download = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--ssl-reqd",
            "--insecure",
            "--user",
            &login,
            "--output",
            downloaded.to_string_lossy().as_ref(),
            &url,
        ])
        .output()
        .unwrap();
    assert!(
        download.status.success(),
        "{}",
        String::from_utf8_lossy(&download.stderr)
    );
    assert_eq!(
        std::fs::read(&downloaded).unwrap(),
        b"openpanel ftp roundtrip"
    );
    let upload_source = tempfile::tempdir().unwrap();
    let oversized = upload_source.path().join("oversized.bin");
    std::fs::write(&oversized, vec![b'x'; 2_048]).unwrap();
    let oversized_url = format!("ftp://{bind}/oversized.bin");
    let over_limit = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--ssl-reqd",
            "--insecure",
            "--user",
            &login,
            "--upload-file",
            oversized.to_string_lossy().as_ref(),
            &oversized_url,
        ])
        .output()
        .unwrap();
    assert!(!over_limit.status.success());
    assert!(!root.path().join("oversized.bin").exists());
    let traversal = std::process::Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--max-time",
            "10",
            "--ssl-reqd",
            "--insecure",
            "--user",
            &login,
            &format!("ftp://{bind}/../../etc/passwd"),
        ])
        .output()
        .unwrap();
    assert!(!traversal.status.success());
    supervisor.join().await;
}
