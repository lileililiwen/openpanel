## ADDED Requirements

### Requirement: Certificate Aggregate

The ssl bounded context SHALL model a `Certificate` aggregate with
fields:

- `id` — UUID v4
- `domain` — the site's `primary_domain` (TEXT, UNIQUE)
- `source` — enum: `Acme`, `Manual`, `SelfSigned`
- `issuer` — common name of the issuing CA (TEXT)
- `valid_from` — `DateTime<Utc>`
- `valid_to` — `DateTime<Utc>`
- `key_type` — e.g. `ECDSA-P256` / `RSA-2048`
- `status` — enum: `Active`, `Expiring`, `Expired`, `Revoked`
- `cert_pem` — PEM-encoded leaf certificate (TEXT)
- `chain_pem` — PEM-encoded intermediate chain (TEXT, may be empty for
  self-signed)
- `key_pem` — AES-GCM ciphertext of the PEM-encoded private key (BLOB)
- `force_https` — bool, default `true` (port-80 → :443 redirect)
- `acme_endpoint` — optional TEXT (only for `source = Acme`):
  staging or production
- `created_at`, `renewed_at` — `DateTime<Utc>`
- `last_error` — optional TEXT (last issuance / renewal error)

`CertificateRepository` MUST support insert / find_by_domain /
list / update / delete and MUST treat `domain` as UNIQUE.

#### Scenario: Insert a certificate

- **WHEN** `CertificateRepository::insert(cert)` is called with a
  cert whose domain already exists
- **THEN** the operation returns `RepoError::UniqueViolation` and no
  row is written.

#### Scenario: Status transitions

- **WHEN** `now > valid_to`
- **THEN** the cert's status MUST be `Expired`.
- **WHEN** `valid_to - now < 30 days`
- **THEN** the cert's status MUST be `Expiring` (the renewal window).

### Requirement: ACME HTTP-01 Issuance

The ssl context SHALL issue Let's Encrypt certificates via the
ACME HTTP-01 challenge using the `rustls-acme` client. Issuance MUST
be configurable between the Let's Encrypt **staging** and
**production** endpoints, with **staging as the default** for safety.

The panel SHALL run an HTTP-01 challenge server on `127.0.0.1:9080`
that serves `/.well-known/acme-challenge/<token>` responses. Every
nginx port-80 vhost SHALL contain a `location ^~
/.well-known/acme-challenge/ { proxy_pass http://127.0.0.1:9080; }`
block forwarding challenge requests to the local server.

#### Scenario: Staging issuance

- **WHEN** `POST /api/v1/ssl/certificates/acme` is called with
  `domain = "example.com"` and the panel is configured with the
  default staging endpoint
- **THEN** the panel contacts Let's Encrypt staging, completes the
  HTTP-01 challenge via the local server, stores the resulting
  certificate with `source = Acme`, `acme_endpoint = staging`, and
  returns the certificate metadata.

#### Scenario: Challenge unreachable

- **WHEN** the domain's port 80 is not reachable from the Internet
  during issuance
- **THEN** the ACME client reports an HTTP-01 challenge failure, the
  certificate is NOT stored, and the API returns
  `SslError::AcmeChallenge("...")` with the captured ACME error.

#### Scenario: Production opt-in

- **WHEN** `config.ssl.acme.production = true`
- **THEN** the ACME client uses
  `https://acme-v02.api.letsencrypt.org/directory` and the resulting
  cert is publicly trusted.

### Requirement: Manual Certificate Upload

The ssl context SHALL accept a manual PEM upload (cert + optional
chain + private key) for a domain. The private key MUST be stored
encrypted at rest (AES-GCM under the panel's master key). The API
MUST NOT return the private key in any response.

#### Scenario: Upload a complete PEM bundle

- **WHEN** `POST /api/v1/ssl/certificates/manual` is called with
  `{domain, cert_pem, chain_pem, key_pem}`
- **THEN** the panel parses all three blocks (cert, chain, key) with
  `x509-parser` and `rustls-pemfile`, validates that the private key
  matches the certificate's public key, stores them under
  `source = Manual`, and returns the cert metadata.

#### Scenario: Key does not match certificate

- **WHEN** the uploaded `key_pem` does not match the uploaded
  `cert_pem`
- **THEN** the upload is rejected with `SslError::KeyMismatch` and
  no row is written.

#### Scenario: Expiry in the past

- **WHEN** the uploaded cert has `valid_to < now`
- **THEN** the upload is rejected with `SslError::Expired` and no row
  is written.

### Requirement: Self-Signed Certificate Generation

The ssl context SHALL generate a self-signed certificate for a domain
on demand using `rcgen` (pure-Rust). The generated cert MUST have a
365-day validity window starting at `now`.

#### Scenario: Generate self-signed

- **WHEN** `POST /api/v1/ssl/certificates/self-signed` is called with
  `domain = "internal.example.com"`
- **THEN** the panel generates an ECDSA-P256 key pair, creates a
  self-signed leaf certificate valid for 365 days, stores it with
  `source = SelfSigned`, and returns the metadata.

### Requirement: Auto-Renewal Scheduler

The ssl context SHALL register a `SslRenewalTask` background task on
the existing `JobSupervisor`. The task MUST run daily and renew every
ACME-issued certificate whose `valid_to - now < 30 days`.

Renewal MUST use the same `AcmeClient` machinery as initial issuance
(so staging / production config is honored) and MUST write back the
updated `cert_pem`, `chain_pem`, `valid_from`, `valid_to`,
`renewed_at`, and reset `last_error` on success.

#### Scenario: Renewal due

- **WHEN** the daily scheduler runs and finds a cert expiring in 25
  days with `source = Acme`
- **THEN** it re-issues via ACME, writes the new cert back to the DB,
  updates `renewed_at = now`, and triggers an nginx reload so the new
  cert is served immediately.

#### Scenario: Renewal failure

- **WHEN** ACME renewal fails (e.g. transient network / rate-limit)
- **THEN** the task records the error in `last_error` and leaves the
  existing cert in place. The next daily tick retries.

#### Scenario: Manual certs are not renewed

- **WHEN** the scheduler scans certs
- **THEN** it MUST skip rows where `source ∈ {Manual, SelfSigned}`
  (those have no automated renewal path).

### Requirement: nginx TLS Integration

The sites context SHALL extend its nginx render to emit a TLS vhost
on port 443 when the domain has an active certificate. The render
MUST emit a Mozilla-modern TLS profile (TLSv1.2+ only, modern ciphers,
HSTS when `force_https` is on).

When SSL is active, the port-80 vhost MUST either:
  1. Redirect all traffic to HTTPS with
     `return 301 https://$host$request_uri;` (when `force_https` is on,
     the default), OR
  2. Serve plain HTTP (when `force_https` is explicitly off).

The render MUST run `nginx -t` after any write; on failure, the prior
state MUST be restored and `SiteError::NginxTest` returned.

#### Scenario: Active cert → TLS vhost present

- **WHEN** the sites context renders a site with an `Active`
  certificate
- **THEN** the generated nginx config contains a second `server { … }`
  block with `listen 443 ssl http2;`,
  `ssl_certificate <path>;`,
  `ssl_certificate_key <path>;`,
  the Mozilla-modern `ssl_protocols` / `ssl_ciphers` directives, and
  an HSTS header when `force_https` is on.

#### Scenario: Force-HTTPS on (default)

- **WHEN** the site has an active cert and `force_https = true`
  (the default)
- **THEN** the port-80 vhost emits
  `return 301 https://$host$request_uri;` and contains NO other
  `location` blocks.

#### Scenario: Force-HTTPS off

- **WHEN** `force_https` is explicitly disabled
- **THEN** the port-80 vhost serves the site over HTTP exactly as it
  would without SSL (acme-challenge proxy block excepted).

#### Scenario: Stale cert removed

- **WHEN** a cert is revoked / deleted and the site still exists
- **THEN** the next nginx render removes the `:443` vhost and either
  emits the HTTP vhost or the force-https redirect based on the new
  state.

### Requirement: ACME HTTP-01 Challenge Server

The panel SHALL run a tiny axum HTTP server on `127.0.0.1:9080` whose
sole responsibility is answering HTTP-01 challenges for active ACME
issuances. The server MUST only respond on `127.0.0.1` (not
`0.0.0.0`) and MUST be started by `SslModule` at boot.

#### Scenario: Challenge token served

- **WHEN** Let's Encrypt requests
  `http://example.com/.well-known/acme-challenge/<token>`
- **THEN** nginx forwards the request to `127.0.0.1:9080`, which
  returns `200 OK` with the `key_authorization` for the active
  issuance of `example.com`.

#### Scenario: Unknown token

- **WHEN** the challenge server receives a request for a token it did
  not register
- **THEN** it returns `404 Not Found` and logs the request at
  `info` level (so a misconfigured proxy surfaces in logs).

#### Scenario: Server not bound to public interface

- **WHEN** the challenge server starts
- **THEN** `TcpListener::bind("127.0.0.1:9080")` succeeds and the
  listener is reachable ONLY from localhost.

### Requirement: Private Key Confidentiality

The ssl context MUST treat private keys as secrets:

- Persist `key_pem` as AES-G-GCM ciphertext (same machinery as the
  databases module's MySQL password storage).
- NEVER include `key_pem` (plaintext OR ciphertext) in any API
  response.
- NEVER log the plaintext key, even at `debug` level.

#### Scenario: API responses omit the key

- **WHEN** `GET /api/v1/ssl/certificates/{domain}` returns metadata
- **THEN** the response contains `cert_pem`, `chain_pem`,
  `valid_from`, `valid_to`, `issuer`, `status`, `force_https` but
  NOT any private-key material.

#### Scenario: Encrypted at rest

- **WHEN** the row is inspected directly in SQLite
- **THEN** the `key_pem` column contains opaque ciphertext bytes, not
  valid PEM.

### Requirement: CLI Surface

The `openpanel` CLI SHALL gain a top-level `ssl` command group:

- `openpanel ssl list`
- `openpanel ssl issue <domain> [--production]`
- `openpanel ssl upload <domain> --cert <pem> [--chain <pem>]
  --key <pem>`
- `openpanel ssl self-signed <domain>`
- `openpanel ssl revoke <domain>`
- `openpanel ssl renew <domain>`

Each subcommand MUST call into the same `SslService` methods the HTTP
API uses, so behavior is identical.

#### Scenario: Issue from the CLI

- **WHEN** `openpanel ssl issue example.com --production` is run
- **THEN** the CLI exits 0, prints the certificate metadata (issuer,
  valid_from, valid_to) to stdout, and the cert is now active in the
  panel's DB.

#### Scenario: Invalid PEM rejected

- **WHEN** `openpanel ssl upload example.com --cert bad.pem --key
  bad.pem` is run with a non-PEM file
- **THEN** the CLI exits non-zero and prints a clear "could not parse
  PEM" message.