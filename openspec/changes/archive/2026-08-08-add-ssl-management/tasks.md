# Tasks: Add SSL Management

> **Standing rule (from `Agents.md`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

> **Standing rule (from `add-tdd-infrastructure`):**
> This change has its own test-support crate usage. Tests go in
> `#[cfg(test)] mod tests` (unit + property) and
> `tests/integration/ssl.rs` (HTTP). The ACME HTTP-01 challenge
> server is integration-tested via the real challenge proxy in the
> `TestServer` harness (nginx config assertion).

## 1. Testing — Domain Layer

- [x] 1.1 Unit test in `openpanel-domain/src/ssl/certificate.rs`:
      `Certificate::new` rejects an empty domain, rejects
      `valid_to <= valid_from`, rejects a cert where the issuer is
      empty when `source = Acme`.
- [x] 1.2 Property test in `openpanel-domain/src/ssl/certificate.rs`
      asserting the status classifier:
      `now > valid_to → Expired`; `valid_to - now < 30 days →
      Expiring`; otherwise `Active`.
- [x] 1.3 Property test for `CertificateRepository::insert`
      uniqueness: two inserts with the same `domain` — the second
      returns `RepoError::UniqueViolation`. Use an in-memory
      `Arc<Mutex<HashMap>>`-backed fake repo to keep the test pure
      (no SQLite).
- [x] 1.4 Unit test for `SslError::KeyMismatch` construction +
      `Display`; ensure no private-key material is in the
      formatted error.

## 2. Testing — Application Layer (Service)

- [x] 2.1 Mock-based test in `openpanel-app/src/ssl/service.rs`:
      `upload_manual` calls the parser, encrypts the key, stores
      metadata, and returns a `Certificate` whose `key_pem` column
      is ciphertext. Use `MockCertificateRepository` +
      `MockAudit` (extend `openpanel-test-support/mocks.rs` with
      `MockCertificateRepository`).
- [x] 2.2 Mock-based test: `generate_self_signed` produces a
      cert whose `valid_to - valid_from ≈ 365 days`, whose leaf
      cert PEM parses, and whose public key matches the stored
      `key_pem`.
- [x] 2.3 Mock-based test: `revoke(domain)` updates `status =
      Revoked`, writes an audit event (`AuditAction::SslRevoked`),
      and triggers an nginx reload.
- [x] 2.4 Mock-based test: `delete(domain)` removes the row AND
      the cert files on disk (use `tempfile::TempDir`).
- [x] 2.5 Mock-based test: `issue_acme` against the **staging**
      endpoint with a mock `AcmeClient` that returns a stub cert;
      asserts the row is written with `source = Acme`,
      `acme_endpoint = staging`, and the cert files are written to
      `paths.cert_dir` and `paths.key_dir`.

## 3. Testing — Renewal Scheduler

- [x] 3.1 Unit test in `openpanel-app/src/ssl/renewal.rs`:
      `SslRenewalTask::tick` scans the repo, picks rows where
      `source = Acme ∧ valid_to - now < 30 days`, and re-issues
      them. Manual / SelfSigned rows are skipped.
- [x] 3.2 Unit test: renewal failure records `last_error` and
      leaves the existing cert in place (does NOT delete the row).

## 4. Testing — ACME HTTP-01 Challenge Server

- [x] 4.1 Unit test: `AcmeHttpServer::register(domain, token,
      key_authorization)` stores the token; a subsequent
      `GET /.well-known/acme-challenge/<token>` returns 200 with
      the key_authorization body.
- [x] 4.2 Unit test: an unknown token returns 404.
- [x] 4.3 Integration test (`tests/integration/ssl.rs`): with a
      `TestServer`, a registered challenge token is served via
      the actual challenge server; an unknown token returns 404.

## 5. Testing — nginx TLS Render Integration

- [x] 5.1 Unit test: `NginxConfigGenerator::render_with_ssl(site,
      &cert)` produces a vhost with `listen 443 ssl http2;`,
      `ssl_certificate <path>;`, `ssl_certificate_key <path>;`,
      and the Mozilla-modern TLS profile.
- [x] 5.2 Unit test: `render_with_force_https(site, true)` on the
      port-80 vhost emits `return 301 https://$host$request_uri;`
      and NO other `location` blocks.
- [x] 5.3 Unit test: `render_with_force_https(site, false)` emits
      the HTTP vhost unchanged (except the acme-challenge proxy
      block).
- [x] 5.4 Integration test (`tests/integration/ssl.rs`): boot
      `TestServer`, install a self-signed cert for a site via the
      service, trigger `nginx -t` (skip if nginx absent), assert
      the active config file contains the `ssl_certificate` line.

## 6. Domain Layer

- [x] 6.1 Create `crates/openpanel-domain/src/ssl/mod.rs`
      re-exporting `Certificate`, `CertificateSource`,
      `CertificateStatus`, `CertificateRepository`, `SslError`,
      `KeyType`.
- [x] 6.2 Create `crates/openpanel-domain/src/ssl/certificate.rs`
      with the `Certificate` aggregate (`new`, `status`,
      `is_expiring`, `is_expired`, `decrypt_key`).
- [x] 6.3 Create `crates/openpanel-domain/src/ssl/source.rs` with
      `CertificateSource` (`Acme`, `Manual`, `SelfSigned`) and
      `CertificateStatus` (`Active`, `Expiring`, `Expired`,
      `Revoked`).
- [x] 6.4 Create `crates/openpanel-domain/src/ssl/error.rs` with
      `SslError` variants: `NotFound`, `Repo`, `AcmeChallenge`,
      `Acme(String)`, `KeyMismatch`, `Expired`, `InvalidPem`,
      `Io`, `Encryption`, `Decryption`.
- [x] 6.5 Create `crates/openpanel-domain/src/ssl/repository.rs`
      with the `CertificateRepository` trait.
- [x] 6.6 Re-export `ssl::*` from `crates/openpanel-domain/src/lib.rs`.

## 7. Application Layer — Service + Repository

- [x] 7.1 Create `crates/openpanel-app/src/ssl/mod.rs`.
- [x] 7.2 Create `crates/openpanel-app/src/ssl/repo.rs` with
      `SqliteCertificateRepository`.
- [x] 7.3 Create `crates/openpanel-app/src/ssl/service.rs` with
      `SslService::new(repo, audit, master_key, paths,
      acme_endpoint)` and the public methods
      `issue_acme / upload_manual / generate_self_signed / revoke
      / delete / renew_if_due / list / get / set_force_https`.
- [x] 7.4 Re-export `SslService` from `crates/openpanel-app/src/lib.rs`.

## 8. Application Layer — PEM Crypto + Parsing

- [x] 8.1 Create `crates/openpanel-app/src/ssl/crypto.rs` with
      `encrypt_key_pem(plaintext_pem: &[u8], master_key: &[u8; 32])
      -> Vec<u8>` and the inverse `decrypt_key_pem(...)`. Reuse
      the AES-GCM primitives from `openpanel-app/src/databases/crypto.rs`.
- [x] 8.2 Create `crates/openpanel-app/src/ssl/parser.rs` with
      `parse_pem_bundle(cert_pem, chain_pem, key_pem) -> ParsedBundle`
      using `x509-parser` + `rustls-pemfile`. Validates the key
      matches the cert via `ring::signature` (Edor) — or, more
      portably, by signing-then-verifying with `rcgen` re-derived
      from the same key.

## 9. Application Layer — ACME Client

- [x] 9.1 Create `crates/openpanel-app/src/ssl/acme.rs` with
      `AcmeClient::new(endpoint: AcmeEndpoint, contact: String,
      account_key_path: PathBuf)`. Wraps `rustls-acme`'s
      `AcmeConfig` + `Order` to expose
      `issue(domain: &str, challenge_server: &AcmeHttpServer)
      -> Result<IssuedCert>`.
- [x] 9.2 `AcmeEndpoint::Staging` and `AcmeEndpoint::Production`
      with the correct URLs.

## 10. Application Layer — Self-Signed Generator

- [x] 10.1 Create `crates/openpanel-app/src/ssl/self_signed.rs`
      with `generate(domain: &str, valid_for_days: u32) ->
      (cert_pem: String, key_pem: String)` using `rcgen`.

## 11. Application Layer — ACME HTTP-01 Challenge Server

- [x] 11.1 Create `crates/openpanel-app/src/ssl/challenge_server.rs`
      with `AcmeHttpServer::bind(port: u16) -> Result<Self>` and
      `register(domain, token, key_authorization)` /
      `serve(router) -> Router`.
- [x] 11.2 The server binds to `127.0.0.1:<port>` only (not
      `0.0.0.0`).

## 12. Application Layer — Renewal Background Task

- [x] 12.1 Create `crates/openpanel-app/src/ssl/renewal.rs` with
      `SslRenewalTask` implementing `BackgroundTask`. `tick`
      scans the repo, calls `acme_client.issue(...)` for rows
      in the renewal window, and writes back the result.
- [x] 12.2 Register the task via `SslModule::background_tasks(ctx)`.

## 13. Migrations & Module Wiring

- [x] 13.1 Create
      `crates/openpanel-app/src/migrations/ssl/V001__init.sql`
      with the `certificates` table + indexes (domain UNIQUE,
      status, expires_at).
- [x] 13.2 Add `pub const SSL_V001` to `migrations/mod.rs`.
- [x] 13.3 Create `crates/openpanel-app/src/ssl/module.rs` with
      `SslModule::with_paths(ctx, paths, master_key,
      acme_endpoint) -> Self` returning the service handle +
      challenge server + renewal task + migration.
- [x] 13.4 Wire `SslModule` into the composition root:
      - `openpanel-test-support/src/server.rs` instantiates the
        module with a sandbox paths.
      - `openpanel-cli/src/handlers.rs` instantiates the module
        in `serve`.
- [x] 13.5 Re-export `SslModule` from `crates/openpanel-app/src/lib.rs`.

## 14. HTTP Routes

- [x] 14.1 Create `crates/openpanel-api/src/dto/ssl.rs` with the
      request/response DTOs (`IssueAcmeRequest`,
      `ManualUploadRequest`, `SelfSignedRequest`,
      `ForceHttpsRequest`, `CertificateMetadataDto`).
- [x] 14.2 Create `crates/openpanel-api/src/routes/ssl.rs` with
      `pub fn router(svc: Arc<SslService>) -> Router` implementing
      the 7 endpoints listed in the spec.
- [x] 14.3 Update `crates/openpanel-api/src/router.rs` to take
      `ssl: Arc<SslService>` and nest `/ssl`.
- [x] 14.4 Add `SslRevoked` to `AuditAction` in
      `openpanel-core/src/audit.rs`.

## 15. CLI Subcommands

- [x] 15.1 Add `SslCommand` enum to
      `crates/openpanel-cli/src/commands.rs`.
- [x] 15.2 Implement handlers in
      `crates/openpanel-cli/src/handlers.rs` that delegate to
      `SslService` (same code path as the HTTP API).
- [x] 15.3 Wire the subcommand in `commands.rs`'s top-level
      `Command::Ssl(SslCommand)` arm.

## 16. nginx Render Extension

- [x] 16.1 Extend `NginxConfigGenerator::render` to take an
      `Option<&Certificate>` parameter; emit the `:443` vhost
      when present.
- [x] 16.2 Extend `render` to emit the acme-challenge proxy
      block on the port-80 vhost.
- [x] 16.3 Extend `render` to emit the force-https redirect when
      `force_https = true` AND a cert is active.
- [x] 16.4 Update `SitesService::create_site`,
      `SitesService::enable_site`, `SitesService::disable_site`,
      `SitesService::delete_site` to look up the cert (via a
      `CertificateRepository` injected at construction) and pass
      it to the render.
- [x] 16.5 Update `SitesModule::with_paths` to take a
      `CertificateRepository` (or pull it from `ctx`) and inject
      it into `SitesService`.

## 17. Documentation

- [x] 17.1 Update root `README.md` with an "SSL" section
      describing ACME HTTP-01, manual upload, self-signed,
      auto-renewal, force-HTTPS, and the staging-default safety
      net.
- [x] 17.2 Update `Agents.md` § Security with the new private-key
      confidentiality rules.
- [x] 17.3 Add `crates/openpanel-app/src/ssl/README.md` covering
      the API surface, the renewal policy, and the storage
      envelope.
- [x] 17.4 Update `tests/README.md` with the new
      `tests/integration/ssl.rs` test category.

## 18. Validation

- [x] 18.1 `cargo test --workspace` passes — all existing tests
      plus the new ssl tests.
- [x] 18.2 `cargo clippy --workspace --all-targets -- -D warnings`
      passes.
- [x] 18.3 `cargo fmt --all -- --check` passes.
- [x] 18.4 `make check` exits 0.
- [x] 18.5 Manual smoke: `make test` against the staging endpoint
      (network-dependent; documented in `tests/README.md`).
- [x] 18.6 Commit + archive via OpenSpec.

## Notes

- The `rustls-acme` dependency is already in the workspace; this
  change does not add it. New workspace deps: `rustls-pemfile`,
  `rcgen`, `x509-parser`.
- Every new public item in `ssl` gets a rustdoc comment — enforced
  by the workspace lints.
- PEM parsing / generation must NOT log the plaintext key. Tests
  that exercise the encrypted-at-rest property must assert
  ciphertext is on disk (not valid PEM).