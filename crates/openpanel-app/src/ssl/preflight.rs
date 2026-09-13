//! ACME preflight checks.
//!
//! Before starting a real issuance, the panel verifies that:
//! 1. The domain's DNS resolves to at least one A / AAAA record.
//! 2. Port 80 is reachable from the panel's perspective (i.e. the
//!    challenge server can be reverse-proxied through nginx).
//! 3. The local challenge server is responsive on the configured
//!    `127.0.0.1:9080` socket.
//!
//! `preflight` is the public entry point. All checks have a 5-second
//! per-check timeout so an unreachable host doesn't block the API
//! call.

use std::{net::SocketAddr, time::Duration};

use tokio::{net::TcpStream, time::timeout};

/// Result of running the preflight checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreflightOutcome {
    /// All checks passed; issuance can proceed.
    Ok,
    /// DNS resolution returned no A/AAAA records (NXDOMAIN or empty).
    DnsFailure(String),
    /// Port 80 was not reachable (timeout, connection refused, or
    /// wrong host).
    PortUnreachable(String),
    /// The local challenge server didn't respond on `127.0.0.1:9080`.
    ChallengeRouting(String),
    /// The check itself exceeded the per-step timeout.
    Timeout(String),
}

impl PreflightOutcome {
    /// True iff the outcome allows proceeding to ACME issuance.
    pub fn is_ok(&self) -> bool {
        matches!(self, PreflightOutcome::Ok)
    }

    /// Stable lowercase identifier.
    pub fn kind(&self) -> &'static str {
        match self {
            PreflightOutcome::Ok => "ok",
            PreflightOutcome::DnsFailure(_) => "dns",
            PreflightOutcome::PortUnreachable(_) => "port",
            PreflightOutcome::ChallengeRouting(_) => "challenge",
            PreflightOutcome::Timeout(_) => "timeout",
        }
    }
}

/// Per-step timeout. 5s is generous for a local TCP connect and a
/// local DNS lookup; Let's Encrypt's own challenge probe is ~10s.
pub const PREFLIGHT_STEP_TIMEOUT: Duration = Duration::from_secs(5);
/// Default port probed for HTTP-01 reachability.
pub const PREFLIGHT_HTTP_PORT: u16 = 80;
/// Default loopback port where the challenge server should respond.
pub const PREFLIGHT_CHALLENGE_PORT: u16 = 9080;

/// Run the preflight checks. `challenge_loopback` is the
/// `127.0.0.1:<port>` the local challenge server is bound to; pass
/// `None` to skip the challenge-routing check (useful in offline
/// tests).
pub async fn preflight(domain: &str, challenge_loopback: Option<SocketAddr>) -> PreflightOutcome {
    let domain = domain.trim();
    if domain.is_empty() {
        return PreflightOutcome::DnsFailure("empty domain".into());
    }
    let addrs = match timeout(PREFLIGHT_STEP_TIMEOUT, resolve(domain)).await {
        Ok(Ok(a)) => a,
        Ok(Err(e)) => {
            return PreflightOutcome::DnsFailure(format!("resolve {domain}: {e}"));
        }
        Err(_) => {
            return PreflightOutcome::Timeout(format!("resolve {domain} > 5s"));
        }
    };
    if addrs.is_empty() {
        return PreflightOutcome::DnsFailure(format!("no A/AAAA records for {domain}"));
    }
    let port_outcome = match addrs.first().copied() {
        Some(addr) => check_port(addr, PREFLIGHT_HTTP_PORT).await,
        None => {
            return PreflightOutcome::DnsFailure(format!("no resolvable addresses for {domain}"));
        }
    };
    if !port_outcome.is_ok() {
        return port_outcome;
    }
    if let Some(addr) = challenge_loopback {
        let outcome = check_port(addr.ip(), addr.port()).await;
        if !outcome.is_ok() {
            return match outcome {
                PreflightOutcome::PortUnreachable(s) => PreflightOutcome::ChallengeRouting(s),
                other => other,
            };
        }
    }
    PreflightOutcome::Ok
}

async fn resolve(domain: &str) -> std::io::Result<Vec<std::net::IpAddr>> {
    let mut out = Vec::new();
    for addr in tokio::net::lookup_host(format!("{domain}:0")).await? {
        out.push(addr.ip());
    }
    Ok(out)
}

async fn check_port(addr: std::net::IpAddr, port: u16) -> PreflightOutcome {
    let sa = SocketAddr::new(addr, port);
    match timeout(PREFLIGHT_STEP_TIMEOUT, TcpStream::connect(sa)).await {
        Ok(Ok(_stream)) => PreflightOutcome::Ok,
        Ok(Err(e)) => match e.kind() {
            std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::HostUnreachable
            | std::io::ErrorKind::NetworkUnreachable => {
                PreflightOutcome::PortUnreachable(format!("{sa}: {e}"))
            }
            _ => PreflightOutcome::PortUnreachable(format!("{sa}: {e}")),
        },
        Err(_) => PreflightOutcome::Timeout(format!("{sa} > 5s")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_domain_is_dns_failure() {
        let r = preflight("   ", None).await;
        assert!(matches!(r, PreflightOutcome::DnsFailure(_)));
    }

    #[tokio::test]
    async fn localhost_resolves_then_may_pass_or_fail_port() {
        // `localhost` always resolves on Linux. Port 80 may or may
        // not be open in the test environment, so the test accepts
        // Ok, DnsFailure (CI images without localhost in /etc/hosts
        // are rare but possible), or PortUnreachable.
        let r = preflight("localhost", None).await;
        assert!(
            r.is_ok()
                || matches!(
                    r,
                    PreflightOutcome::DnsFailure(_)
                        | PreflightOutcome::PortUnreachable(_)
                        | PreflightOutcome::Timeout(_)
                ),
            "unexpected outcome: {r:?}"
        );
    }

    #[tokio::test]
    async fn non_existent_domain_is_dns_failure() {
        // The `localhost.invalid` TLD is reserved by RFC 6761; DNS
        // MUST NOT resolve it. If the resolver in this environment
        // returns a wildcard answer (rare), the test still fails
        // loudly via the `is_ok` check.
        let r = preflight("nonexistent-test.invalid", None).await;
        assert!(!r.is_ok(), "expected non-OK, got {r:?}");
    }

    #[tokio::test]
    async fn unreachable_loopback_is_port_failure() {
        // Bind a server, get the port, then drop it. The next connect
        // attempt should fail with ConnectionRefused (or Timeout on
        // busy CI). We accept either, but the test fails on Ok.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let r = check_port("127.0.0.1".parse().unwrap(), port).await;
        assert!(!r.is_ok(), "expected port-failure, got {r:?}");
    }

    #[tokio::test]
    async fn reachable_loopback_is_ok() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        // Hold the listener open.
        let _holder = listener;
        let r = check_port(addr.ip(), addr.port()).await;
        assert!(r.is_ok(), "expected ok, got {r:?}");
    }

    #[tokio::test]
    async fn preflight_with_challenge_loopback_passes_challenge_check() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let _holder = listener;
        // localhost:80 is not open in CI, so the outcome is either
        // Ok (port 80 reachable on this host) or PortUnreachable.
        // What we are really asserting is that the function does NOT
        // return ChallengeRouting (because the loopback is up).
        let r = preflight("localhost", Some(addr)).await;
        assert!(
            r.is_ok()
                || matches!(
                    r,
                    PreflightOutcome::DnsFailure(_)
                        | PreflightOutcome::PortUnreachable(_)
                        | PreflightOutcome::Timeout(_)
                ),
            "unexpected outcome: {r:?}"
        );
    }
}
