# ssl-production-lifecycle Specification

## Purpose
TBD - created by archiving change complete-production-acme-lifecycle. Update Purpose after archive.
## Requirements
### Requirement: Complete HTTP-01 Issuance

The production ACME adapter MUST complete directory discovery, account/order
creation, HTTP-01 challenge polling, finalization, and certificate retrieval
using the configured rustls-acme version.

#### Scenario: Staging issuance succeeds

- **WHEN** an authorized caller requests staging issuance for a reachable domain
- **THEN** the certificate is stored as an ACME certificate with staging
  endpoint metadata and encrypted key material.

### Requirement: Fail-Closed Issuance

Issuance MUST not persist a certificate when challenge, validation, polling,
finalization, or certificate parsing fails.

#### Scenario: Challenge unreachable

- **WHEN** the public challenge cannot reach the panel
- **THEN** issuance returns a stable actionable error and no certificate row
  is written.

### Requirement: Safe Production Opt-In

Production ACME MUST require explicit configuration and every issuance status
MUST identify staging versus production without exposing account secrets.

#### Scenario: Default endpoint

- **WHEN** no production opt-in is configured
- **THEN** the adapter uses staging.

### Requirement: Renewal Is Durable

Renewal MUST record due, success, failure, retry time, and last error, preserve
the previous valid certificate on failure, and reload nginx only after success.

#### Scenario: Renewal fails

- **WHEN** a due renewal encounters a transient ACME error
- **THEN** the existing certificate remains active, the failure is redacted and
  persisted, and a later retry is scheduled.

### Requirement: Recovery Is Visible

Web, API, and CLI surfaces MUST show certificate state, last safe error,
reachability guidance, and retry action without returning private keys.

#### Scenario: Operator views failed issuance

- **WHEN** issuance fails
- **THEN** the operator sees a recoverable failure state with no key or token
  material.

### Requirement: Bounded Issuance With Exponential Backoff

The issuance state machine MUST cap polling attempts at 6, grow the backoff
from 2s to a 60s ceiling, and surface a `Timeout` classified error when the
order does not become `Ready` or `Valid` within the cap. Each successful
poll must record the attempt count and the next backoff; the unregister on the
challenge server must run on every exit path.

#### Scenario: Order never becomes Ready

- **WHEN** the order stays in `Pending` after the bounded attempts
- **THEN** the issuance returns `IssuanceError::Timeout`, the challenge token
  is unregistered, and no certificate row is written.

### Requirement: ACME Errors Are Classified

Raw ACME error strings (network, `no http-01 challenge`, `rate limited`,
timeout, HTTP, JSON, JOSE) MUST map to a stable `IssuanceError` variant
(`Challenge`, `RateLimited`, `Unreachable`, `Invalid`, `Timeout`, `Network`,
`Internal`) and from there to a stable `SslError` variant. The mapping MUST
be pure (no I/O, no clock) so the offline `MockAcmeClient` and the
production `RustlsAcmeClient` produce the same classification for the same
input.

#### Scenario: Rate-limited CA response

- **WHEN** the CA returns a `urn:ietf:params:acme:error:rateLimited` problem
- **THEN** the issuance returns `SslError::AcmeRateLimited` and the renewal
  scheduler honours the 24-hour backoff.

#### Scenario: Redacted error text

- **WHEN** an error string contains `Authorization: Bearer …`, a PEM block,
  or a JWS-shaped nonce
- **THEN** every persisted or audit-logged copy replaces the secret with
  `<redacted>` and the prefix `<pem-block redacted>` and contains no key,
  token, or JOSE material.

### Requirement: ACME Preflight Surfaces DNS And Port Reachability

Before starting issuance, the panel MUST run a preflight that resolves the
domain's A/AAAA records and TCP-connects to port 80 with a 5-second
per-step timeout. The result (`Ok`, `DnsFailure`, `PortUnreachable`,
`ChallengeRouting`, `Timeout`) MUST be readable through the web/API/CLI
without exposing private-key material.

#### Scenario: Domain has no A record

- **WHEN** the domain's DNS returns no A/AAAA records
- **THEN** the preflight returns `DnsFailure` and issuance MUST NOT be
  attempted.

#### Scenario: Port 80 is unreachable

- **WHEN** the port-80 TCP connect fails or times out within 5s
- **THEN** the preflight returns `PortUnreachable` and issuance MUST NOT be
  attempted.

### Requirement: Renewal Honours 24-Hour Backoff

When an ACME issuance attempt fails with a transient error (`RateLimited`,
`Unreachable`, `Timeout`, `Network`), the renewal scheduler MUST NOT retry
the same certificate within 24 hours. Manual / SelfSigned certificates are
unaffected by this rule.

#### Scenario: Backoff after rate limit

- **WHEN** a renewal for `example.com` returns `AcmeRateLimited` at T0
- **THEN** the next daily tick (T0 + 24h or later) is the earliest retry
  and the cert row records `last_attempt_at = T0`.

### Requirement: Nginx Reload Is Gated On Successful Renewal

A successful renewal MUST trigger an nginx `nginx -t && nginx -s reload`.
If `nginx -t` fails, the previous certificate files MUST remain in place
and the renewal MUST be reported as failed (the in-DB cert is not
overwritten with a half-broken file).

#### Scenario: nginx -t fails after renewal

- **WHEN** `nginx -t` returns non-zero on the renewed cert files
- **THEN** the previous cert files are restored, the new cert row is
  discarded, and the renewal records the test failure as `last_error`.

