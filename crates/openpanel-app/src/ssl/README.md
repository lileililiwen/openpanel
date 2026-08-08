# `openpanel-app/src/ssl/`

TLS bounded context: certificate lifecycle (ACME HTTP-01, manual
upload, self-signed), automatic renewal, nginx TLS integration.

## Public surface

| Item | Purpose |
|---|---|
| `SslService` | Application service — manual upload, self-signed generation, ACME issuance, revoke, delete, force-HTTPS toggle, renew-now. |
| `AcmeClient` | Trait that the service depends on. `RustlsAcmeClient` is the real adapter (drives the `rustls-acme 0.13` HTTP-01 lifecycle); `MockAcmeClient` is the test double. |
| `AcmeEndpoint` | `Staging` (default, safe) / `Production`. |
| `AcmeHttpServer` | Tiny axum server bound to `127.0.0.1:9080`. nginx port-80 vhosts forward `/.well-known/acme-challenge/<token>` to it. |
| `SslRenewalTask` | `BackgroundTask` that wakes every 24 hours and re-issues ACME certs whose `valid_to - now < 30 days`. Registered via `SslModule::background_tasks`. |
| `SslPaths` | Filesystem paths for the leaf cert (`cert_dir`) and private key (`key_dir`). Production defaults: `/etc/openpanel/ssl/{certs,keys}`. |
| `SslModule` | Composition-root wiring: builds the service, challenge server, renewal task, and V001 migration. |
| `SqliteCertificateRepository` | SQLite-backed adapter for `CertificateRepository`. |

## Renewal policy

`SslRenewalTask::run` loops every `24 h` on `tokio::select!` over
`tokio::time::sleep(24h)` and `shutdown.notified()`. On each tick:

1. List every `Certificate`.
2. Skip `Manual` and `SelfSigned` rows — only `Acme` is renewable.
3. For each `Acme` row whose `valid_to - now <= 30 days`, call
   `SslService::issue_acme(domain)`.
4. On success the row is overwritten (renewal updates
   `valid_from`/`valid_to`/`cert_pem`/`chain_pem`/`renewed_at`).
5. On failure, `last_error` is set and the next tick retries.

## Storage envelope

Private keys are stored in the `certificates.key_ciphertext` column as
**AES-256-GCM ciphertext** using the same master key / nonce format as
the databases module's password storage:

```
hex(nonce) || ":" || hex(ciphertext)
```

The plaintext key never appears in the DB. The plaintext key is only
written to disk at `SslPaths::key_path(domain)` for nginx to read,
with `0600` permissions on Unix. The API never returns the key in any
response — only metadata (`issuer`, `valid_from`, `valid_to`,
`status`, `force_https`, `source`).

## ACME HTTP-01 flow

1. `SslService::issue_acme(domain)` calls `AcmeClient::issue`.
2. The client discovers the directory, creates (or reuses) an ACME
   account, creates a new order for the domain.
3. Pulls the authz, finds the HTTP-01 challenge, computes the
   `key_authorization`, registers it with the local
   `AcmeHttpServer` under the challenge token.
4. Tells the CA "I'm ready" (`account.challenge(...)`).
5. Polls the order until `Ready`, builds a CSR with rcgen,
   `finalize`s, polls until `Valid`, downloads the chain.
6. `SslService::persist_issued` stores the new cert material
   (re-uses `SslPaths`).

The local challenge server is **the only listener on
`127.0.0.1:9080`** — nginx's port-80 vhost has a
`location ^~ /.well-known/acme-challenge/ { proxy_pass
http://127.0.0.1:9080; }` block that forwards requests to it. This
keeps nginx in charge of the `:80`/`virtual-host` boundary while the
ACME server stays purely local.

## Force-HTTPS 301 redirect

When a cert is active, `SslService::set_force_https(domain, true)`
(default) flips the per-site redirect. The nginx render emits:

```
location ^~ /.well-known/acme-challenge/ { proxy_pass ...; }
location / { return 301 https://$host$request_uri; }
```

The `^~` prefix match has priority over the unnamed `location /`, so
renewals still work even with force-HTTPS on.

## Configuration

| Env / config | Effect |
|---|---|
| `OPENPANEL__SSL__CONTACT_EMAIL` | ACME account contact (`mailto:[email protected]` by default). |
| `OPENPANEL__SSL__PATHS__CERT_DIR` | Override the leaf-cert output directory (default `/etc/openpanel/ssl/certs`). |
| `OPENPANEL__SSL__PATHS__KEY_DIR` | Override the private-key output directory (default `/etc/openpanel/ssl/keys`). |
| `OPENPANEL__DATABASE__MASTER_KEY` | The same 32-byte base64 key used to encrypt passwords in the databases module; reused for cert-key ciphertext. |

## Tests

- `openpanel-domain/src/ssl/certificate.rs` — 9 unit + property tests.
- `openpanel-app/src/ssl/{crypto,parser,self_signed,challenge_server,renewal}.rs` — unit tests covering round-trip encryption, key/cert matching via rcgen + x509-parser SPKI, ACME challenge server, renewal-window classifier.
- `tests/integration/ssl.rs` — 4 HTTP tests (`list empty`, `self-signed create/list/delete`, `unauthenticated rejected`, `unknown domain 404`).
- `crates/openpanel-cli/tests/cli/ssl.rs` — 2 CLI tests (`list`, `self-signed creates/lists/revokes`).

The `RustlsAcmeClient::issue` body is intentionally a skeleton
returning `SslError::Acme("RustlsAcmeClient is a skeleton ...")`. The
trait, mock, and full lifecycle scaffolding are in place; the real
driver is deferred to a follow-up change with network tests against
Let's Encrypt **staging** (the default).