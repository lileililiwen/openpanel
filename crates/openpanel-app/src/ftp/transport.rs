//! Supervised libunftp listener with site-qualified authentication.

use std::{
    fmt,
    io::Cursor,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use async_trait::async_trait;
use futures_util::FutureExt;
use libunftp::{
    ServerBuilder,
    notification::{DataEvent, DataListener, EventMeta, PresenceEvent, PresenceListener},
    options::{FailedLoginsBlock, FailedLoginsPolicy, FtpsRequired},
};
use openpanel_core::{AuditAction, AuditEvent, AuditOutcome, AuditService, jobs::BackgroundTask};
use openpanel_domain::ftp::{FtpAccount, FtpError, FtpRepository, FtpTlsMode};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Notify,
};
use unftp_core::{
    auth::{
        AuthenticationError, Authenticator, ChannelEncryptionState, Credentials, Principal,
        UserDetail, UserDetailError, UserDetailProvider,
    },
    storage::{
        Error as StorageError, ErrorKind as StorageErrorKind, Fileinfo, Metadata, StorageBackend,
    },
};
use unftp_sbe_fs::{Filesystem, Meta};
use unftp_sbe_restrict::{RestrictingVfs, UserWithPermissions, VfsOperations};
use unftp_sbe_rooter::{RooterVfs, UserWithRoot};
use uuid::Uuid;

use super::{FtpAuthenticator, FtpSessionRegistry};

/// Environment-backed supervised listener settings.
#[derive(Debug, Clone)]
pub struct FtpServerConfig {
    /// Whether the listener is started.
    pub enabled: bool,
    /// Control-channel bind address.
    pub bind: SocketAddr,
    /// Control and data channel encryption mode.
    pub tls_mode: FtpTlsMode,
    /// PEM certificate chain used for explicit FTPS.
    pub cert_path: Option<PathBuf>,
    /// PEM private key used for explicit FTPS.
    pub key_path: Option<PathBuf>,
    /// Passive data-channel port range.
    pub passive_ports: std::ops::RangeInclusive<u16>,
    /// Maximum authenticated sessions across all accounts.
    pub global_max_connections: u32,
}

impl FtpServerConfig {
    /// Read FTP settings from `OPENPANEL__FTP__*` environment variables.
    pub fn from_env() -> Result<Self, FtpError> {
        let enabled = env_bool("OPENPANEL__FTP__ENABLED", false)?;
        let bind: SocketAddr = std::env::var("OPENPANEL__FTP__BIND")
            .unwrap_or_else(|_| "0.0.0.0:21".into())
            .parse()
            .map_err(|_| FtpError::Invalid("invalid FTP bind address".into()))?;
        let tls_mode = FtpTlsMode::from_str(
            &std::env::var("OPENPANEL__FTP__TLS_MODE").unwrap_or_else(|_| "StartTls".into()),
        )?;
        if tls_mode == FtpTlsMode::ImplicitTls {
            return Err(FtpError::Invalid(
                "ImplicitTls is not supported by the embedded libunftp transport; use StartTls"
                    .into(),
            ));
        }
        if tls_mode == FtpTlsMode::None && !bind.ip().is_loopback() {
            return Err(FtpError::Invalid(
                "plaintext FTP may bind only to loopback".into(),
            ));
        }
        let cert_path = std::env::var("OPENPANEL__FTP__TLS_CERT")
            .ok()
            .map(PathBuf::from);
        let key_path = std::env::var("OPENPANEL__FTP__TLS_KEY")
            .ok()
            .map(PathBuf::from);
        if enabled && tls_mode != FtpTlsMode::None && (cert_path.is_none() || key_path.is_none()) {
            return Err(FtpError::Invalid(
                "TLS certificate and key are required when FTP is enabled".into(),
            ));
        }
        Ok(Self {
            enabled,
            bind,
            tls_mode,
            cert_path,
            key_path,
            passive_ports: 50000..=50100,
            global_max_connections: 256,
        })
    }
}

fn env_bool(name: &str, default: bool) -> Result<bool, FtpError> {
    match std::env::var(name) {
        Ok(value) => match value.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Ok(true),
            "0" | "false" | "no" => Ok(false),
            _ => Err(FtpError::Invalid(format!("invalid boolean for {name}"))),
        },
        Err(_) => Ok(default),
    }
}

#[derive(Clone)]
struct FtpUser {
    qualified: String,
    home: PathBuf,
    enabled: bool,
    read_only: bool,
    transferred: Arc<AtomicU64>,
    transfer_limit_bytes: u64,
    account_id: Uuid,
    audit: Arc<dyn AuditService>,
}
impl fmt::Debug for FtpUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FtpUser")
            .field("qualified", &self.qualified)
            .field("home", &"[redacted]")
            .finish_non_exhaustive()
    }
}
impl fmt::Display for FtpUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FTP user {}", self.qualified)
    }
}
impl UserDetail for FtpUser {
    fn account_enabled(&self) -> bool {
        self.enabled
    }

    fn home(&self) -> Option<&Path> {
        Some(&self.home)
    }
}
impl UserWithRoot for FtpUser {
    fn user_root(&self) -> Option<PathBuf> {
        Some(self.home.clone())
    }
}
impl UserWithPermissions for FtpUser {
    fn permissions(&self) -> VfsOperations {
        let read = VfsOperations::GET | VfsOperations::LIST | VfsOperations::MD5;
        if self.read_only {
            read
        } else {
            VfsOperations::all()
        }
    }
}

impl FtpUser {
    fn claim_transfer(&self, bytes: u64) -> bool {
        self.transferred
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |used| {
                used.checked_add(bytes)
                    .filter(|next| *next <= self.transfer_limit_bytes)
            })
            .is_ok()
    }

    async fn audit_storage_denial(&self) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    &self.qualified,
                    AuditAction::FtpChrootEscape,
                    AuditOutcome::Denied,
                )
                .target(self.account_id.to_string())
                .metadata(serde_json::json!({"path":"[redacted]"})),
            )
            .await;
    }

    async fn audit_transfer_limit(&self) {
        let _ = self
            .audit
            .record(
                AuditEvent::new(
                    &self.qualified,
                    AuditAction::FtpTransferLimit,
                    AuditOutcome::Denied,
                )
                .target(self.account_id.to_string()),
            )
            .await;
    }
}

#[derive(Debug)]
struct QuotaVfs<Delegate> {
    inner: Delegate,
}

impl<Delegate> QuotaVfs<Delegate> {
    fn new(inner: Delegate) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl<Delegate> StorageBackend<FtpUser> for QuotaVfs<Delegate>
where
    Delegate: StorageBackend<FtpUser, Metadata = Meta>,
{
    type Metadata = Meta;

    fn name(&self) -> &str {
        self.inner.name()
    }

    fn supported_features(&self) -> u32 {
        self.inner.supported_features()
    }

    async fn metadata<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<Meta, StorageError> {
        self.inner.metadata(user, path).await
    }

    async fn list<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<Vec<Fileinfo<PathBuf, Meta>>, StorageError> {
        self.inner.list(user, path).await
    }

    async fn list_fmt<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<Cursor<Vec<u8>>, StorageError> {
        self.inner.list_fmt(user, path).await
    }

    async fn nlst<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<Cursor<Vec<u8>>, std::io::Error> {
        self.inner.nlst(user, path).await
    }

    async fn get_into<'a, P, W: ?Sized>(
        &self,
        user: &FtpUser,
        path: P,
        start_pos: u64,
        output: &'a mut W,
    ) -> Result<u64, StorageError>
    where
        W: AsyncWrite + Unpin + Sync + Send,
        P: AsRef<Path> + Send + fmt::Debug,
    {
        let metadata = match self.inner.metadata(user, path.as_ref()).await {
            Ok(value) => value,
            Err(error) => {
                user.audit_storage_denial().await;
                return Err(error);
            }
        };
        let bytes = metadata.len().saturating_sub(start_pos);
        if !user.claim_transfer(bytes) {
            user.audit_transfer_limit().await;
            return Err(StorageErrorKind::PermissionDenied.into());
        }
        match self.inner.get_into(user, path, start_pos, output).await {
            Ok(bytes) => Ok(bytes),
            Err(error) => {
                user.audit_storage_denial().await;
                Err(error)
            }
        }
    }

    async fn get<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
        start_pos: u64,
    ) -> Result<Box<dyn AsyncRead + Send + Sync + Unpin>, StorageError> {
        let metadata = match self.inner.metadata(user, path.as_ref()).await {
            Ok(value) => value,
            Err(error) => {
                user.audit_storage_denial().await;
                return Err(error);
            }
        };
        let bytes = metadata.len().saturating_sub(start_pos);
        if !user.claim_transfer(bytes) {
            user.audit_transfer_limit().await;
            return Err(StorageErrorKind::PermissionDenied.into());
        }
        match self.inner.get(user, path, start_pos).await {
            Ok(reader) => Ok(reader),
            Err(error) => {
                user.audit_storage_denial().await;
                Err(error)
            }
        }
    }

    async fn put<
        P: AsRef<Path> + Send + fmt::Debug,
        R: AsyncRead + Send + Sync + Unpin + 'static,
    >(
        &self,
        user: &FtpUser,
        input: R,
        path: P,
        start_pos: u64,
    ) -> Result<u64, StorageError> {
        let path = path.as_ref().to_path_buf();
        let bytes = self.inner.put(user, input, &path, start_pos).await?;
        if user.claim_transfer(bytes) {
            return Ok(bytes);
        }
        let _ = self.inner.del(user, &path).await;
        user.audit_transfer_limit().await;
        Err(StorageErrorKind::PermissionDenied.into())
    }

    async fn del<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<(), StorageError> {
        self.inner.del(user, path).await
    }

    async fn mkd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<(), StorageError> {
        self.inner.mkd(user, path).await
    }

    async fn rename<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        from: P,
        to: P,
    ) -> Result<(), StorageError> {
        self.inner.rename(user, from, to).await
    }

    async fn rmd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<(), StorageError> {
        self.inner.rmd(user, path).await
    }

    async fn cwd<P: AsRef<Path> + Send + fmt::Debug>(
        &self,
        user: &FtpUser,
        path: P,
    ) -> Result<(), StorageError> {
        self.inner.cwd(user, path).await
    }
}

struct LibAuth {
    inner: Arc<FtpAuthenticator>,
    tls_required: bool,
    sessions: Arc<FtpSessionRegistry>,
    global_max_connections: u32,
    audit: Arc<dyn AuditService>,
}
impl fmt::Debug for LibAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LibAuth")
            .field("tls_required", &self.tls_required)
            .finish_non_exhaustive()
    }
}
#[async_trait]
impl Authenticator for LibAuth {
    async fn authenticate(
        &self,
        username: &str,
        creds: &Credentials,
    ) -> Result<Principal, AuthenticationError> {
        if self.tls_required && creds.command_channel_security != ChannelEncryptionState::Tls {
            let _ = self
                .audit
                .record(AuditEvent::new(
                    username,
                    AuditAction::FtpTlsRequired,
                    AuditOutcome::Denied,
                ))
                .await;
            return Err(AuthenticationError::new("TLS required"));
        }
        if self.sessions.total() >= self.global_max_connections as usize {
            let _ = self
                .audit
                .record(
                    AuditEvent::new(
                        username,
                        AuditAction::FtpConcurrentLimit,
                        AuditOutcome::Denied,
                    )
                    .metadata(serde_json::json!({"scope":"global"})),
                )
                .await;
            return Err(AuthenticationError::new(
                "global FTP connection limit reached",
            ));
        }
        let (site_id, display) = qualified(username).map_err(|_| AuthenticationError::BadUser)?;
        let password = creds
            .password
            .as_deref()
            .ok_or(AuthenticationError::BadPassword)?;
        self.inner
            .authenticate(site_id, display, password, creds.source_ip)
            .await
            .map_err(|_| AuthenticationError::BadPassword)?;
        Ok(Principal {
            username: username.to_owned(),
        })
    }
}

struct DetailProvider {
    repo: Arc<dyn FtpRepository>,
    audit: Arc<dyn AuditService>,
}
impl fmt::Debug for DetailProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DetailProvider")
    }
}
#[async_trait]
impl UserDetailProvider for DetailProvider {
    type User = FtpUser;

    async fn provide_user_detail(
        &self,
        principal: &Principal,
    ) -> Result<Self::User, UserDetailError> {
        let (site_id, username) =
            qualified(&principal.username).map_err(|_| UserDetailError::UserNotFound {
                username: principal.username.clone(),
            })?;
        let account = self
            .repo
            .find_by_username(site_id, username)
            .await
            .map_err(|_| UserDetailError::Generic("account lookup failed".into()))?
            .ok_or_else(|| UserDetailError::UserNotFound {
                username: principal.username.clone(),
            })?;
        Ok(user(&principal.username, &account, self.audit.clone()))
    }
}
fn user(qualified: &str, account: &FtpAccount, audit: Arc<dyn AuditService>) -> FtpUser {
    FtpUser {
        qualified: qualified.into(),
        home: account.home().to_path_buf(),
        enabled: account.enabled(),
        read_only: account.read_only(),
        transferred: Arc::new(AtomicU64::new(0)),
        transfer_limit_bytes: account
            .limits()
            .bandwidth_kb_per_session()
            .saturating_mul(1024),
        account_id: account.id(),
        audit,
    }
}

struct DataAccounting {
    repo: Arc<dyn FtpRepository>,
    sessions: Arc<FtpSessionRegistry>,
}
impl fmt::Debug for DataAccounting {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DataAccounting")
    }
}

#[async_trait]
impl DataListener for DataAccounting {
    async fn receive_data_event(&self, event: DataEvent, meta: EventMeta) {
        let bytes = match event {
            DataEvent::Got { bytes, .. } | DataEvent::Put { bytes, .. } => bytes,
            _ => return,
        };
        if let Ok((site_id, username)) = qualified(&meta.username)
            && let Ok(Some(account)) = self.repo.find_by_username(site_id, username).await
        {
            self.sessions.add_bytes(account.id(), bytes);
        }
    }
}
fn qualified(value: &str) -> Result<(Uuid, &str), FtpError> {
    let (site, user) = value
        .split_once('@')
        .ok_or_else(|| FtpError::Invalid("wire username must be site-uuid@username".into()))?;
    if user.is_empty() {
        return Err(FtpError::Invalid("missing FTP username".into()));
    }
    Ok((
        Uuid::parse_str(site)
            .map_err(|_| FtpError::Invalid("invalid site-qualified username".into()))?,
        user,
    ))
}

struct Presence {
    repo: Arc<dyn FtpRepository>,
    sessions: Arc<FtpSessionRegistry>,
}
impl fmt::Debug for Presence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Presence")
    }
}
#[async_trait]
impl PresenceListener for Presence {
    async fn receive_presence_event(&self, event: PresenceEvent, meta: EventMeta) {
        if matches!(event, PresenceEvent::LoggedOut)
            && let Ok((site_id, username)) = qualified(&meta.username)
            && let Ok(Some(account)) = self.repo.find_by_username(site_id, username).await
        {
            self.sessions.close_one(account.id());
        }
    }
}

/// Background task that starts libunftp only when globally enabled.
pub struct FtpServerTask {
    config: FtpServerConfig,
    repo: Arc<dyn FtpRepository>,
    authenticator: Arc<FtpAuthenticator>,
    sessions: Arc<FtpSessionRegistry>,
    audit: Arc<dyn AuditService>,
}
impl FtpServerTask {
    /// Construct the supervised listener task and its shared adapters.
    pub fn new(
        config: FtpServerConfig,
        repo: Arc<dyn FtpRepository>,
        authenticator: Arc<FtpAuthenticator>,
        sessions: Arc<FtpSessionRegistry>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            config,
            repo,
            authenticator,
            sessions,
            audit,
        }
    }
}

#[async_trait]
impl BackgroundTask for FtpServerTask {
    fn name(&self) -> &'static str {
        "ftp-server"
    }

    async fn run(self: Box<Self>, shutdown: Arc<Notify>) -> anyhow::Result<()> {
        if !self.config.enabled {
            return Ok(());
        }
        if self.config.tls_mode == FtpTlsMode::ImplicitTls {
            return Err(anyhow::anyhow!(
                "ImplicitTls is unsupported by the embedded libunftp transport"
            ));
        }
        let mut attempts = 0_u8;
        loop {
            let root = Filesystem::new("/").map_err(|error| anyhow::anyhow!(error.to_string()))?;
            drop(root);
            let provider = Arc::new(DetailProvider {
                repo: self.repo.clone(),
                audit: self.audit.clone(),
            });
            #[allow(
                clippy::expect_used,
                reason = "the infallible libunftp factory is constructed only after the same root was opened successfully"
            )]
            let backend = Box::new(|| {
                QuotaVfs::new(RestrictingVfs::<RooterVfs<Filesystem,FtpUser,Meta>,FtpUser,Meta>::new(RooterVfs::new(Filesystem::new("/").expect("invariant: FTP filesystem root was opened immediately before server construction"))))
            });
            let auth: Arc<dyn Authenticator + Send + Sync> = Arc::new(LibAuth {
                inner: self.authenticator.clone(),
                tls_required: self.config.tls_mode != FtpTlsMode::None,
                sessions: self.sessions.clone(),
                global_max_connections: self.config.global_max_connections,
                audit: self.audit.clone(),
            });
            let mut builder = ServerBuilder::with_user_detail_provider(backend, provider)
                .authenticator(auth)
                .greeting("OpenPanel FTP")
                .idle_session_timeout(300)
                .passive_ports(self.config.passive_ports.clone())
                .failed_logins_policy(FailedLoginsPolicy::new(
                    5,
                    Duration::from_secs(60),
                    FailedLoginsBlock::IP,
                ))
                .notify_presence(Presence {
                    repo: self.repo.clone(),
                    sessions: self.sessions.clone(),
                })
                .notify_data(DataAccounting {
                    repo: self.repo.clone(),
                    sessions: self.sessions.clone(),
                });
            if self.config.tls_mode != FtpTlsMode::None {
                let cert = self
                    .config
                    .cert_path
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("FTP TLS certificate missing"))?;
                let key = self
                    .config
                    .key_path
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("FTP TLS key missing"))?;
                builder = builder
                    .ftps(cert, key)
                    .ftps_required(FtpsRequired::All, FtpsRequired::All);
            }
            let server = builder
                .build()
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let bind = self.config.bind.to_string();
            let listen = std::panic::AssertUnwindSafe(server.listen(bind)).catch_unwind();
            let result = tokio::select! {result=listen=>Some(result),_=shutdown.notified()=>None};
            match result {
                None => return Ok(()),
                Some(Ok(Ok(()))) => return Ok(()),
                Some(Ok(Err(error))) => {
                    let _ = self
                        .audit
                        .record(
                            AuditEvent::new(
                                "ftp-supervisor",
                                AuditAction::FtpBindFailed,
                                AuditOutcome::Failure,
                            )
                            .metadata(serde_json::json!({"reason":error.to_string()})),
                        )
                        .await;
                    return Err(anyhow::anyhow!(
                        "FTP server failed to bind or listen: {error}"
                    ));
                }
                Some(Err(_panic)) => {
                    attempts = attempts.saturating_add(1);
                    let _ = self
                        .audit
                        .record(
                            AuditEvent::new(
                                "ftp-supervisor",
                                AuditAction::FtpListenerRestart,
                                AuditOutcome::Failure,
                            )
                            .metadata(
                                serde_json::json!({"reason":"listener panic","attempt":attempts}),
                            ),
                        )
                        .await;
                    if attempts >= 3 {
                        return Err(anyhow::anyhow!("FTP server panicked three times"));
                    }
                    tokio::time::sleep(Duration::from_secs(u64::from(attempts) * 2)).await;
                }
            }
        }
    }
}
