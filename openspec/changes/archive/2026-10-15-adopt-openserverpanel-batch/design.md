# Design: adopt-openserverpanel-batch

## Context

Four independent-but-batched capabilities, all previously flagged by
our own gap tracking (`docs/competitive-gap-analysis.md`,
`HANDOFF.md`, `ssl-production-lifecycle` Purpose) and validated by the
OSPanel comparison:

- `software_center` already models per-entry `versions`
  (`domain/src/software_center/recipe.rs` `VersionSpec`) but installed
  state is single-row: `software_components(id PRIMARY KEY, version…)`
  (`app/src/migrations/software_center/V001__init.sql`) and the
  install APIs (`mod.rs` `preview_install`, `install_artifact`,
  `start_artifact_install`) take no version.
- `ssl/acme.rs` `RustlsAcmeClient::issue` is an explicit stub; the
  event bridge into our `AcmeHttpServer` is the only missing piece
  (classification/redaction/backoff already live in `issuance_state`).
- No WebDAV code exists (only roadmap prose). FTP gives us the full
  account-shaped template: `domain/ftp` (`FtpPath` chroot VO,
  `FtpLimits`, argon2 `FtpAuthenticator`), `app/ftp` service+repo,
  `api/routes/ftp.rs`, `web/src/ftp.rs` aggregate page.
- `site_staging` slots have no mail hooks; the one real user-facing
  send path is `webmail_client/service.rs::send` (lettre
  `AsyncSmtpTransport`, same shape as `notifications/adapter.rs`).

Approval: the human principal instructed "proposal and implement it"
on 2026-10-15 for exactly this batch; this design records that
session directive as the review gate.

## Goals / Non-Goals

**Goals:** co-installable component versions with version-targeted
operations; live HTTP-01 issuance completing the spec that already
exists; jailed WebDAV accounts; staging-scoped mail capture with an
inbox.

**Non-Goals:** see proposal Non-goals (no catalog additions, no
LOCK/ACL WebDAV, no production mail path changes, no runtime-switch
UX).

## Decisions

### D1 — Software Center: PK-rebuild migration to `(id, version)`

Rebuild `software_components` with `PRIMARY KEY (id, version)` in a new
`V003__versioned_installs.sql`; existing single rows migrate to their
current version (or `''` head row). Install/preview/uninstall APIs gain
`version: Option<String>`; `None` keeps today's behaviour (latest).
Update badge SQL (`store.rs:650`) compares each version row against
`latest_version`.

- Alternative: side table `component_installs(id, version)` FK →
  rejected: dual-write drift with the existing ensure/sync writer at
  `mod.rs:2519`, and every read path would need a join.
- Alternative: encode `"id@version"` in the id column — rejected:
  breaks catalog joins (`software_entry_versions`) and audit refs.

### D2 — ACME: in-crate event bridge, challenge lookup delegation

Implement `RustlsAcmeClient::issue` by driving `rustls_acme::AcmeState`
(0.13, `AcmeConfig::directory_lets_encrypt(prod)` + `UseChallenge::
Http01`) as a Stream of `Result<EventOk, EventError>`:

1. `AcmeHttpServer` gains an attachable resolver-lookup hook
   (`attach_token_lookup(Arc<dyn Fn(&str) -> Option<String> + Send +
   Sync>)`); its existing router falls back to the hook after the
   manual map. This reuses the nginx-proxy topology — no second server.
2. A private in-memory `Cache` impl captures cert DER on
   `CertCacheStore` and persists nothing (fail-closed by construction);
   the account key stays process-lifetime (Open Question Q1).
3. Events loop until `DeployedNewCert`; `EventError` is stringified
   into the existing `classify_acme_error` → `map_issuance_error`
   pipeline; polling bounds reuse `MAX_POLL_ATTEMPTS`/`backoff_for_attempt`.
4. Testability seam: the state-driving loop is extracted behind a
   private `AcmeOrderDriver` trait with the real rustls-acme
   implementation and a scriptable test double, so `issue()` is
   unit-testable offline (GREEN evidence without touching Let's
   Encrypt). Existing `MockAcmeClient` and all offline tests unchanged.

- Alternative: mount rustls-acme's own tower/axum challenge service —
  rejected: bypasses `AcmeHttpServer`'s register/unregister contract
  that `SslService`, renewal task and tests already depend on.

### D3 — WebDAV: new bounded context, FS-backed axum router

`domain/src/webdav`: `WebDavAccount` (site_id, username,
`home_abs: FtpPath`, `read_only`, `enabled`, argon2 hash) reusing
`openpanel_domain::ftp::FtpPath` for chroot containment, repository
trait, error enum. `app/src/webdav`: `WebDavService` (CRUD + auth,
mirroring `FtpService`), SQLite `V001__init.sql`, `module.rs`. The DAV
verb layer is an axum `Router` (OPTIONS, PROPFIND depth 0/1, GET, HEAD,
PUT, DELETE, MKCOL, MOVE, COPY, PROPPATCH→empty 207) with Basic auth
against the account store; it is exported for the composition root to
merge at `/dav/{account}/…` — no new port, no new listener. Path
resolution is chroot-validated (reject `..`, symlinks escaping home,
NFC-normalize). Read-only accounts: mutating verbs → 403.

- Alternative: ship SFTPGo via container-runtime — rejected: adds an
  external dependency and credential sync surface; our file model is
  already chroot-FS based and testable with `tower::ServiceExt`.
- Alternative: libunftp-based DAV — rejected: libunftp is FTP-only.

### D4 — Mail capture: port + webmail send hook

`domain/src/mail_capture`: `CapturedMessage` entity +
`MailCapturePort` trait (`enabled_for_site`, `store`, `list`, `read`,
`purge`). `app/src/mail_capture`: SQLite-backed service + module
(`mail_capture_messages` table; body encrypted at rest with the same
AES-256-GCM secret storage as other secrets; never logged). Hook point:
`webmail_client/service.rs::send` checks the port before SMTP — when
the sender's site has an active staging slot with capture ON, the
message is stored and a success result is returned with
`captured: true`; otherwise the path is byte-identical to today.
Toggle lives with the staging slot (spec delta `site-staging`); capture
rows are purged on `delete_slot`. Web surface: capture inbox under
`/mail` site view (list/subject/from/date, read detail, purge-all).

- Alternative: intercept at MTA queue (`MtaQueuePort`) — rejected:
  it is queue-read-only by spec and changing Postfix routing for
  staging is heavier and untestable offline.

## Explore & Reuse (mandated)

- Reused, not reinvented: `domain/ftp::FtpPath` + `FtpLimits` shape +
  argon2 `FtpAuthenticator` pattern (D3); `ssl/issuance_state`
  classification/redaction/backoff (D2); `AcmeHttpServer` router +
  nginx proxy topology (D2); `domain/software_center::VersionSpec` +
  `StorefrontEntry.versions` (D1); lettre transport pattern from
  `notifications/adapter.rs` (D4); module shape + `ModuleRegistry`
  registration, api route + web aggregate-page + migration conventions
  from the `ftp` context.
- New code is unavoidable only for: DAV verb layer, capture storage,
  version-keyed component rows.

## Risks / Trade-offs

- [PK-rebuild migration touches live install-tracking rows] →
  migration is a pure copy-then-swap with `COALESCE(version,'')`;
  ensure-pipeline tests assert head-row continuity before archive.
- [rustls-acme 0.13 event loop is long-lived; a stalled order could
  hang issuance] → hard cap via `MAX_POLL_ATTEMPTS` +
  `backoff_for_attempt`, emit `IssuanceError::Timeout` and
  `unregister`/detach the lookup hook on every exit path.
- [No live LE exercise in CI] → mock-ACME integration test in
  `crates/openpanel-app/tests/acme_mock.rs` (scripted
  directory/newNonce/newAccount/newOrder/authz/challenge/finalize
  endpoints, self-signed leaf PEM) proves the real wire path; staging
  endpoint remains default so first real issuance is opt-in.
- [Basic-auth WebDAV over plain HTTP leaks creds] → accounts carry a
  TLS-required flag; panel serves `/dav` only behind the existing TLS
  listener; spec scenario pins 401-before-auth on every verb.
- [Capture inbox grows unbounded] → retention cap constant
  (default 1000/site) with FIFO eviction, spec'd and tested.

## Migration Plan

1. DB: `V003` software-center rebuild (idempotent, copy+swap),
   `webdav` + `mail_capture` V001 creates.
2. Rollback: v0.x — new tables are additive; `software_components`
   rollback not provided (panel upgrade), documented in HANDOFF.
3. No API removals; version fields optional → old CLI/API payloads
   keep working.

## Open Questions

- Q1: persist the ACME account key across restarts (needs secret-store
  wiring) — deferred; in-memory means re-registration per process,
  acceptable for staging-default.
- Q2: per-version apt pinning granularity beyond `VersionSpec.packages`
  — follow-up if operators request side-by-side same-major installs.
