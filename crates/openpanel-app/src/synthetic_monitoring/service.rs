//! Synthetic monitoring services: HTTP / TCP / SSL probe ports,
//! a check runner, and a throttle-aware scheduler.

use std::sync::Arc;

use chrono::Utc;
use openpanel_core::AuditService;
use openpanel_domain::{
    CheckResult, CheckStatus, CheckType, Role, SyntheticCheck, SyntheticError, SyntheticRepository,
    User,
};
use uuid::Uuid;

use crate::synthetic_monitoring::SqliteSyntheticRepository;

/// Result of a single probe — the inputs the runner needs to
/// classify the result and persist it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckOutcome {
    /// Latency in milliseconds.
    pub latency_ms: u32,
    /// HTTP status (HTTP probes only).
    pub http_status: Option<u16>,
    /// Days remaining on the certificate (SSL probes only).
    pub cert_days_remaining: Option<u32>,
    /// Final classification.
    pub status: CheckStatus,
    /// Optional detail string.
    pub message: String,
}

/// Port that runs an HTTP probe. The default implementation uses
/// `reqwest`; tests use `RecordingProbe`.
#[async_trait::async_trait]
pub trait HttpProbe: Send + Sync + 'static {
    /// Issue an HTTP GET against the target and return the
    /// observed status + latency in milliseconds.
    async fn get(&self, url: &str, timeout_secs: u32) -> Result<(u16, u32), SyntheticError>;
}

/// Port that runs a TCP connect probe.
#[async_trait::async_trait]
pub trait TcpProbe: Send + Sync + 'static {
    /// Open a TCP connection to `host:port` and return the
    /// latency in milliseconds.
    async fn connect(&self, host: &str, port: u16, timeout_secs: u32) -> Result<u32, SyntheticError>;
}

/// Port that inspects a TLS certificate.
#[async_trait::async_trait]
pub trait SslExpiryInspector: Send + Sync + 'static {
    /// Return the number of days until `host`'s certificate
    /// expires.
    async fn days_remaining(&self, host: &str, timeout_secs: u32) -> Result<u32, SyntheticError>;
}

/// Recording probe used by tests. Each call records its name and
/// status; the returned tuple is configurable per call.
pub struct RecordingProbe {
    http: std::sync::Mutex<Option<(u16, u32)>>,
    tcp: std::sync::Mutex<Option<u32>>,
    ssl: std::sync::Mutex<Option<u32>>,
    http_calls: std::sync::Mutex<Vec<String>>,
    tcp_calls: std::sync::Mutex<Vec<(String, u16)>>,
    ssl_calls: std::sync::Mutex<Vec<String>>,
}

impl RecordingProbe {
    /// Construct an empty recorder.
    pub fn new() -> Self {
        Self {
            http: std::sync::Mutex::new(None),
            tcp: std::sync::Mutex::new(None),
            ssl: std::sync::Mutex::new(None),
            http_calls: std::sync::Mutex::new(Vec::new()),
            tcp_calls: std::sync::Mutex::new(Vec::new()),
            ssl_calls: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Set the next HTTP response: `(status, latency_ms)`.
    pub fn set_http_response(&self, status: u16, latency_ms: u32) {
        *self.http.lock().expect("http") = Some((status, latency_ms));
    }

    /// Set the next TCP latency in milliseconds.
    pub fn set_tcp_latency(&self, latency_ms: u32) {
        *self.tcp.lock().expect("tcp") = Some(latency_ms);
    }

    /// Set the next SSL days-remaining value.
    pub fn set_ssl_days_remaining(&self, days: u32) {
        *self.ssl.lock().expect("ssl") = Some(days);
    }

    /// Snapshot the recorded HTTP calls.
    pub fn http_calls(&self) -> Vec<String> {
        self.http_calls.lock().expect("http").clone()
    }

    /// Snapshot the recorded TCP calls.
    pub fn tcp_calls(&self) -> Vec<(String, u16)> {
        self.tcp_calls.lock().expect("tcp").clone()
    }

    /// Snapshot the recorded SSL calls.
    pub fn ssl_calls(&self) -> Vec<String> {
        self.ssl_calls.lock().expect("ssl").clone()
    }
}

impl Default for RecordingProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpProbe for RecordingProbe {
    async fn get(&self, url: &str, _timeout: u32) -> Result<(u16, u32), SyntheticError> {
        self.http_calls.lock().expect("http").push(url.to_string());
        self.http
            .lock()
            .expect("http")
            .ok_or_else(|| SyntheticError::Probe("no http response configured".into()))
    }
}

#[async_trait::async_trait]
impl TcpProbe for RecordingProbe {
    async fn connect(&self, host: &str, port: u16, _timeout: u32) -> Result<u32, SyntheticError> {
        self.tcp_calls
            .lock()
            .expect("tcp")
            .push((host.to_string(), port));
        self.tcp
            .lock()
            .expect("tcp")
            .ok_or_else(|| SyntheticError::Probe("no tcp latency configured".into()))
    }
}

#[async_trait::async_trait]
impl SslExpiryInspector for RecordingProbe {
    async fn days_remaining(&self, host: &str, _timeout: u32) -> Result<u32, SyntheticError> {
        self.ssl_calls.lock().expect("ssl").push(host.to_string());
        self.ssl
            .lock()
            .expect("ssl")
            .ok_or_else(|| SyntheticError::Probe("no ssl days configured".into()))
    }
}

/// Runs a single check end-to-end and records the result.
pub struct CheckRunner {
    repo: Arc<SqliteSyntheticRepository>,
    http: Arc<dyn HttpProbe>,
    tcp: Arc<dyn TcpProbe>,
    ssl: Arc<dyn SslExpiryInspector>,
    audit: Arc<dyn AuditService>,
}

impl CheckRunner {
    /// Construct a runner with the default ports.
    pub fn new(
        repo: Arc<SqliteSyntheticRepository>,
        http: Arc<dyn HttpProbe>,
        tcp: Arc<dyn TcpProbe>,
        ssl: Arc<dyn SslExpiryInspector>,
        audit: Arc<dyn AuditService>,
    ) -> Self {
        Self {
            repo,
            http,
            tcp,
            ssl,
            audit,
        }
    }

    /// Run `check` once, persist the result, and return it.
    pub async fn run(&self, check: &SyntheticCheck) -> Result<CheckResult, SyntheticError> {
        let outcome = self.probe(check).await?;
        let status = if outcome.status == CheckStatus::Ok {
            CheckStatus::Ok
        } else {
            outcome.status
        };
        let result = CheckResult {
            id: Uuid::new_v4(),
            check_id: check.id,
            ran_at: Utc::now(),
            latency_ms: outcome.latency_ms,
            http_status: outcome.http_status,
            cert_days_remaining: outcome.cert_days_remaining,
            status,
            message: outcome.message,
        };
        self.repo.save_result(&result).await?;
        self.repo.touch_check(check.id, result.ran_at).await?;
        let _ = self.audit;
        Ok(result)
    }

    async fn probe(&self, check: &SyntheticCheck) -> Result<CheckOutcome, SyntheticError> {
        match check.kind {
            CheckType::Http => self.probe_http(check).await,
            CheckType::Tcp => self.probe_tcp(check).await,
            CheckType::Ssl => self.probe_ssl(check).await,
        }
    }

    async fn probe_http(&self, check: &SyntheticCheck) -> Result<CheckOutcome, SyntheticError> {
        let (status, latency_ms) = self.http.get(&check.target, check.timeout_secs).await?;
        let expected = check.expected_status.unwrap_or(200);
        let result = if status == expected {
            CheckStatus::Ok
        } else if (500..600).contains(&status) {
            CheckStatus::Fail
        } else {
            CheckStatus::Warn
        };
        Ok(CheckOutcome {
            latency_ms,
            http_status: Some(status),
            cert_days_remaining: None,
            status: result,
            message: format!("http {status}"),
        })
    }

    async fn probe_tcp(&self, check: &SyntheticCheck) -> Result<CheckOutcome, SyntheticError> {
        let (host, port) = parse_host_port(&check.target)?;
        let latency_ms = self.tcp.connect(&host, port, check.timeout_secs).await?;
        Ok(CheckOutcome {
            latency_ms,
            http_status: None,
            cert_days_remaining: None,
            status: CheckStatus::Ok,
            message: format!("tcp {host}:{port} ok"),
        })
    }

    async fn probe_ssl(&self, check: &SyntheticCheck) -> Result<CheckOutcome, SyntheticError> {
        let days = self
            .ssl
            .days_remaining(&check.target, check.timeout_secs)
            .await?;
        let status = if days == 0 {
            CheckStatus::Fail
        } else if days <= check.warn_before_days {
            CheckStatus::Warn
        } else {
            CheckStatus::Ok
        };
        Ok(CheckOutcome {
            latency_ms: 0,
            http_status: None,
            cert_days_remaining: Some(days),
            status,
            message: format!("cert expires in {days} days"),
        })
    }
}

fn parse_host_port(target: &str) -> Result<(String, u16), SyntheticError> {
    let (host, port) = target
        .split_once(':')
        .ok_or_else(|| SyntheticError::InvalidTarget("tcp target must be host:port".into()))?;
    let port: u16 = port
        .parse()
        .map_err(|_| SyntheticError::InvalidTarget("tcp port must be a u16".into()))?;
    Ok((host.to_string(), port))
}

/// Throttle-aware scheduler. The scheduler enforces the
/// `throttle_secs` window on every check before dispatching a
/// run, so a forced run inside the window returns
/// `SyntheticError::Throttled` and creates no row.
pub struct ProbeScheduler {
    repo: Arc<SqliteSyntheticRepository>,
    runner: Arc<CheckRunner>,
}

impl ProbeScheduler {
    /// Construct a scheduler.
    pub fn new(repo: Arc<SqliteSyntheticRepository>, runner: Arc<CheckRunner>) -> Self {
        Self { repo, runner }
    }

    /// Enqueue a forced run for `check_id`. Returns
    /// `SyntheticError::Throttled` when the throttle window has
    /// not yet elapsed.
    pub async fn force_run(&self, caller: &User, check_id: Uuid) -> Result<CheckResult, SyntheticError> {
        require_admin(caller)?;
        let check = self
            .repo
            .get_check(check_id)
            .await?
            .ok_or(SyntheticError::CheckNotFound(check_id))?;
        if let Some(last) = check.last_run_at {
            let elapsed = (Utc::now() - last).num_seconds().max(0) as u32;
            if elapsed < check.throttle_secs {
                return Err(SyntheticError::Throttled);
            }
        }
        self.runner.run(&check).await
    }

    /// List all configured checks.
    pub async fn list(&self) -> Result<Vec<SyntheticCheck>, SyntheticError> {
        Ok(self.repo.list_checks().await?)
    }
}

fn require_admin(caller: &User) -> Result<(), SyntheticError> {
    match caller.role() {
        Role::Owner | Role::Admin => Ok(()),
        _ => Err(SyntheticError::Forbidden),
    }
}
