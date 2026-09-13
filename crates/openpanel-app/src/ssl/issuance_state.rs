//! Issuance state machine + ACME error classification for the ssl
//! bounded context.
//!
//! Pure domain — no I/O, no network, no clock reads. Used by both
//! the production `RustlsAcmeClient` and the offline `MockAcmeClient`
//! to surface consistent, redacted errors to the operator.
//!
//! `IssuanceAttempt` is the in-flight state for a single `issue`
//! call. It tracks bounded retries and a redacted error string so the
//! operator sees "rate limited" or "challenge failed" without
//! leaking headers, nonces, or PEM material.
//!
//! The redaction strips any header-like value (e.g. `Authorization: …`),
//! JWS-shaped strings, and PEM blocks before the text reaches
//! `Certificate::last_error` or the audit log.

use std::time::Duration;

use chrono::{DateTime, Utc};

/// Maximum polling attempts during issuance order readiness + finalization.
pub const MAX_POLL_ATTEMPTS: u32 = 6;
/// Initial backoff between poll attempts; doubled up to
/// [`MAX_POLL_BACKOFF`].
pub const INITIAL_POLL_BACKOFF: Duration = Duration::from_secs(2);
/// Upper bound on the exponential backoff.
pub const MAX_POLL_BACKOFF: Duration = Duration::from_secs(60);
/// 24h backoff for the renewal scheduler after a failed attempt.
pub const RENEWAL_RETRY_AFTER: Duration = Duration::from_secs(24 * 60 * 60);

/// Classified outcome of an issuance attempt. Translated by
/// `SslService` into `SslError` variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssuanceError {
    /// The challenge could not be served (no http-01 challenge in the
    /// authorization, or the server-side `Challenge::Invalid`
    /// response). Operator must investigate the domain's
    /// reachability.
    Challenge(String),
    /// The CA returned a `rateLimited` problem. The renewal scheduler
    /// MUST skip the cert for [`RENEWAL_RETRY_AFTER`].
    RateLimited(String),
    /// DNS resolution failed or port 80 was unreachable from the
    /// Internet. Preflight refused to start the issuance.
    Unreachable(String),
    /// The order entered `Invalid` status with a `Problem` document
    /// we don't classify as a challenge / rate-limit failure.
    Invalid(String),
    /// The polling loop exhausted [`MAX_POLL_ATTEMPTS`] before the
    /// order became `Ready` / `Valid`.
    Timeout(String),
    /// A network-layer error (TLS, DNS at request time, HTTP). The
    /// renewal scheduler treats this as transient.
    Network(String),
    /// A catch-all for any other error. Operators should treat this
    /// as "look at the panel logs".
    Internal(String),
}

impl IssuanceError {
    /// Stable lowercase identifier used in API / CLI output and
    /// persisted in `last_error` as a prefix.
    pub fn kind(&self) -> &'static str {
        match self {
            IssuanceError::Challenge(_) => "challenge",
            IssuanceError::RateLimited(_) => "rate_limited",
            IssuanceError::Unreachable(_) => "unreachable",
            IssuanceError::Invalid(_) => "invalid",
            IssuanceError::Timeout(_) => "timeout",
            IssuanceError::Network(_) => "network",
            IssuanceError::Internal(_) => "internal",
        }
    }

    /// Whether the renewal scheduler MUST honour the 24h backoff
    /// after this error. True for `RateLimited`, `Unreachable`,
    /// `Timeout`, and `Network` — anything likely to recur if retried
    /// immediately. False for `Challenge` / `Invalid` (the operator
    /// must fix the domain) and `Internal` (escalate).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            IssuanceError::RateLimited(_)
                | IssuanceError::Unreachable(_)
                | IssuanceError::Timeout(_)
                | IssuanceError::Network(_)
        )
    }

    /// Redact the message for storage. Re-applies the redactor so
    /// classification + redaction are tested together.
    pub fn redact(&self) -> String {
        let raw = match self {
            IssuanceError::Challenge(s)
            | IssuanceError::RateLimited(s)
            | IssuanceError::Unreachable(s)
            | IssuanceError::Invalid(s)
            | IssuanceError::Timeout(s)
            | IssuanceError::Network(s)
            | IssuanceError::Internal(s) => s,
        };
        format!("{}: {}", self.kind(), redact_acme_text(raw))
    }
}

/// Classify a CA-side `Problem` document. Returns `Some(kind)` if the
/// problem type matches a known error category; `None` for "we don't
/// have a special classification for this".
pub fn classify_problem(problem_type: &str) -> Option<IssuanceError> {
    let t = problem_type.trim();
    if t.is_empty() {
        return None;
    }
    // URN-style ACME problem identifiers per RFC 8555 §6.7.
    if t.ends_with(":rateLimited") {
        return Some(IssuanceError::RateLimited(t.into()));
    }
    if t.ends_with(":connection") || t.ends_with(":dns") {
        return Some(IssuanceError::Unreachable(t.into()));
    }
    if t.ends_with(":malformed") || t.ends_with(":serverInternal") {
        return Some(IssuanceError::Internal(t.into()));
    }
    if t.ends_with(":caa") || t.ends_with(":rejectedIdentifier") || t.ends_with(":invalidEmail") {
        return Some(IssuanceError::Invalid(t.into()));
    }
    None
}

/// Classify a raw error string from the ACME adapter into a
/// structured [`IssuanceError`]. This is the function the rest of the
/// codebase uses; it stays pure (no `AcmeError` import) so the
/// offline `MockAcmeClient` and the production `RustlsAcmeClient` can
/// both call it with the message they extracted.
pub fn classify_acme_error(raw: &str) -> IssuanceError {
    let raw = raw.trim();
    if raw.is_empty() {
        return IssuanceError::Internal("empty error".into());
    }
    let lower = raw.to_ascii_lowercase();
    if lower.contains("no http-01 challenge") || lower.contains("nohttp01challenge") {
        return IssuanceError::Challenge("no http-01 challenge in authorization".into());
    }
    if lower.contains("rate limit") || lower.contains("ratelimited") {
        return IssuanceError::RateLimited(raw.into());
    }
    if lower.contains("timeout") || lower.contains("timed out") {
        return IssuanceError::Timeout(raw.into());
    }
    if lower.contains("dns")
        || lower.contains("connection refused")
        || lower.contains("unreachable")
        || lower.contains("network")
    {
        return IssuanceError::Unreachable(raw.into());
    }
    if lower.contains("invalid") && lower.contains("order") {
        return IssuanceError::Invalid(raw.into());
    }
    if lower.contains("auth") && lower.contains("invalid") {
        return IssuanceError::Invalid(raw.into());
    }
    if lower.contains("http request") || lower.contains("io error") {
        return IssuanceError::Network(raw.into());
    }
    IssuanceError::Internal(raw.into())
}

/// In-flight state for a single `issue` call.
#[derive(Debug, Clone)]
pub struct IssuanceAttempt {
    /// When the attempt started.
    pub started_at: DateTime<Utc>,
    /// Number of polls against the CA so far.
    pub attempt_count: u32,
    /// Last redacted error, if any. Reset on success.
    pub last_error_redacted: Option<String>,
}

impl IssuanceAttempt {
    /// Begin a new attempt. `attempt_count` is 0; the caller
    /// increments it after each poll.
    pub fn start(now: DateTime<Utc>) -> Self {
        Self {
            started_at: now,
            attempt_count: 0,
            last_error_redacted: None,
        }
    }

    /// True iff more polls are allowed.
    pub fn can_poll(&self) -> bool {
        self.attempt_count < MAX_POLL_ATTEMPTS
    }

    /// Record a successful poll. Returns the next backoff.
    pub fn record_poll(&mut self) -> Duration {
        self.attempt_count = self.attempt_count.saturating_add(1);
        let factor = 1u32 << self.attempt_count.min(5);
        let backoff = INITIAL_POLL_BACKOFF.saturating_mul(factor);
        backoff.min(MAX_POLL_BACKOFF)
    }

    /// Record a classified error.
    pub fn record_error(&mut self, err: &IssuanceError) {
        self.last_error_redacted = Some(err.redact());
    }

    /// Reset for the next attempt (after a clean success or a fresh
    /// `issue` call).
    pub fn clear(&mut self) {
        self.attempt_count = 0;
        self.last_error_redacted = None;
    }
}

/// Strip header-like secrets, JWS-shaped strings, and PEM blocks from
/// a free-form error message. Idempotent; safe to call multiple
/// times.
pub fn redact_acme_text(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while !rest.is_empty() {
        if let Some(idx) = rest.find("Authorization:") {
            out.push_str(&rest[..idx]);
            out.push_str("Authorization: <redacted>");
            rest = skip_to_newline(&rest[idx + "Authorization:".len()..]);
            continue;
        }
        if let Some(idx) = rest.find("authorization:") {
            out.push_str(&rest[..idx]);
            out.push_str("authorization: <redacted>");
            rest = skip_to_newline(&rest[idx + "authorization:".len()..]);
            continue;
        }
        if let Some(idx) = rest.find("Bearer ") {
            out.push_str(&rest[..idx]);
            out.push_str("Bearer <redacted>");
            rest = skip_to_newline(&rest[idx + "Bearer ".len()..]);
            continue;
        }
        if let Some(idx) = rest.find("nonce=") {
            out.push_str(&rest[..idx]);
            out.push_str("nonce=<redacted>");
            rest = skip_to_newline(&rest[idx + "nonce=".len()..]);
            continue;
        }
        if let Some(idx) = rest.find("-----BEGIN") {
            out.push_str(&rest[..idx]);
            out.push_str("<pem-block redacted>");
            let end_idx = rest.find("-----END").unwrap_or(rest.len());
            let after_end = rest[end_idx..]
                .find('\n')
                .map(|i| end_idx + i + 1)
                .unwrap_or(rest.len());
            rest = &rest[after_end..];
            continue;
        }
        // No more sensitive markers; copy the rest verbatim.
        out.push_str(rest);
        rest = "";
    }
    out.trim().to_string()
}

fn skip_to_newline(s: &str) -> &str {
    match s.find('\n') {
        Some(n) => &s[n + 1..],
        None => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-09T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn classify_empty_returns_internal() {
        assert_eq!(
            classify_acme_error(""),
            IssuanceError::Internal("empty error".into())
        );
        assert_eq!(
            classify_acme_error("   "),
            IssuanceError::Internal("empty error".into())
        );
    }

    #[test]
    fn classify_no_http01_challenge() {
        let e = classify_acme_error("no http-01 challenge found in authorization");
        assert!(matches!(e, IssuanceError::Challenge(_)));
        assert_eq!(e.kind(), "challenge");
    }

    #[test]
    fn classify_rate_limited() {
        let e = classify_acme_error("error: rate limited by Let's Encrypt");
        assert!(matches!(e, IssuanceError::RateLimited(_)));
        assert!(e.is_transient());
    }

    #[test]
    fn classify_timeout() {
        let e = classify_acme_error("operation timed out after 30s");
        assert!(matches!(e, IssuanceError::Timeout(_)));
        assert!(e.is_transient());
    }

    #[test]
    fn classify_dns() {
        let e = classify_acme_error("DNS resolution failed: no A record");
        assert!(matches!(e, IssuanceError::Unreachable(_)));
        assert!(e.is_transient());
    }

    #[test]
    fn classify_unreachable_connection() {
        let e = classify_acme_error("connection refused");
        assert!(matches!(e, IssuanceError::Unreachable(_)));
    }

    #[test]
    fn classify_invalid_order() {
        let e = classify_acme_error("order is invalid: CAA record forbids issuance");
        assert!(matches!(e, IssuanceError::Invalid(_)));
        assert!(!e.is_transient());
    }

    #[test]
    fn classify_http_request() {
        let e = classify_acme_error("http request error: connection reset by peer");
        assert!(matches!(e, IssuanceError::Network(_)));
        assert!(e.is_transient());
    }

    #[test]
    fn classify_unknown_returns_internal() {
        let e = classify_acme_error("cryptic server fault");
        assert!(matches!(e, IssuanceError::Internal(_)));
        assert!(!e.is_transient());
    }

    #[test]
    fn classify_problem_ratelimited() {
        let e = classify_problem("urn:ietf:params:acme:error:rateLimited").unwrap();
        assert!(matches!(e, IssuanceError::RateLimited(_)));
    }

    #[test]
    fn classify_problem_connection() {
        let e = classify_problem("urn:ietf:params:acme:error:connection").unwrap();
        assert!(matches!(e, IssuanceError::Unreachable(_)));
    }

    #[test]
    fn classify_problem_unknown() {
        assert!(classify_problem("urn:ietf:params:acme:error:tls").is_none());
    }

    #[test]
    fn issuance_attempt_can_poll() {
        let mut a = IssuanceAttempt::start(fixed_now());
        assert!(a.can_poll());
        for _ in 0..MAX_POLL_ATTEMPTS {
            a.record_poll();
        }
        assert!(!a.can_poll());
    }

    #[test]
    fn issuance_attempt_backoff_grows_then_caps() {
        let mut a = IssuanceAttempt::start(fixed_now());
        let b1 = a.record_poll();
        let b2 = a.record_poll();
        let b3 = a.record_poll();
        assert!(b2 > b1);
        assert!(b3 >= b2);
        // Cap: many records should not exceed MAX_POLL_BACKOFF.
        for _ in 0..10 {
            a.record_poll();
        }
        let capped = a.record_poll();
        assert!(capped <= MAX_POLL_BACKOFF);
    }

    #[test]
    fn issuance_attempt_records_redacted_error() {
        let mut a = IssuanceAttempt::start(fixed_now());
        a.record_error(&IssuanceError::RateLimited(
            "Authorization: Bearer secret123".into(),
        ));
        let s = a.last_error_redacted.expect("error should be recorded");
        assert!(s.contains("rate_limited"));
        assert!(!s.contains("secret123"));
        assert!(s.contains("<redacted>"));
    }

    #[test]
    fn redact_strips_authorization_header() {
        let r = redact_acme_text("error: Authorization: Bearer abc123 is invalid");
        assert!(!r.contains("abc123"));
        assert!(r.contains("Authorization: <redacted>"));
    }

    #[test]
    fn redact_strips_pem_block() {
        let r = redact_acme_text(
            "failed at -----BEGIN PRIVATE KEY-----\nMIIE...lots of base64\n-----END PRIVATE KEY-----\n next line",
        );
        assert!(!r.contains("MIIE"));
        assert!(r.contains("<pem-block redacted>"));
        assert!(r.contains("next line"));
    }

    #[test]
    fn redact_strips_bearer_token() {
        let r = redact_acme_text("Authorization header Bearer xyz789 is malformed");
        assert!(!r.contains("xyz789"));
    }

    #[test]
    fn redact_idempotent() {
        let r1 = redact_acme_text("Authorization: Bearer foo");
        let r2 = redact_acme_text(&r1);
        assert_eq!(r1, r2);
    }

    #[test]
    fn redact_preserves_benign_text() {
        let r = redact_acme_text("no http-01 challenge found");
        assert_eq!(r, "no http-01 challenge found");
    }
}
