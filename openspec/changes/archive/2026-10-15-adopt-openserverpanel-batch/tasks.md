# Tasks: adopt-openserverpanel-batch

## 1. Testing

- [x] 1.1 Software Center: failing unit tests in `crates/openpanel-app/src/software_center/tests.rs` (+ integration `tests/software_center.rs`): version-targeted preview/install, unknown version rejected, default resolves to latest, two versions coexist in inventory, per-version update badge, uninstall-one-keeps-other. _2026-10-15 GREEN: `cargo test -p openpanel-app --test software_center` 54 passed; `cargo test --test integration software_center` 14 passed._
- [x] 1.2 ACME: failing tests — `AcmeHttpServer` token-lookup hook fallback (`ssl/tests.rs`), scripted `AcmeOrderDriver` success/error/timeout-bridge tests in `ssl/acme.rs` tests (replacing the gated-stub test), mock-ACME-directory integration test `crates/openpanel-app/tests/acme_mock.rs` completing HTTP-01 end-to-end offline. _2026-10-15 GREEN: `cargo test -p openpanel-app --lib ssl` 62 passed (incl. token-lookup hook + scripted driver success/error/timeout); `--test acme_mock` 3 passed offline end-to-end._
- [x] 1.3 WebDAV: failing tests — domain VO/validation unit tests; `app/src/webdav` service CRUD + auth tests; axum router integration tests via `tower::ServiceExt::oneshot`: 401-challenge anonymous, chroot traversal 404, read-only 403 on PUT, PROPFIND depth 0/1 → 207 XML, PUT/GET roundtrip, MKCOL/MOVE/COPY, OPTIONS, LOCK → 501; api routes `/sites/{id}/webdav/accounts` CRUD. _2026-10-15 GREEN: domain `cargo test -p openpanel-domain --lib webdav` 5 passed; `cargo test -p openpanel-app --test webdav` 16 passed (401/chroot/read-only/PROPFIND/verbs/501); `cargo test --test integration webdav` 3 passed._
- [x] 1.4 Mail capture: failing tests — `mail_capture` service (store/list/read/purge, FIFO retention cap, encrypted body at rest), `site_staging` toggle default-off + destroy-purges, `webmail_client::send` divert vs passthrough. _2026-10-15 GREEN: domain `--lib mail_capture` 5 passed; `cargo test -p openpanel-app --test mail_capture` 8 passed; `--lib site_staging` 8 passed; `cargo test --test integration mail_capture` 4 passed._
- [x] 1.5 Governance self-tests: `make test-gates` green with new checker-less capabilities registered; tasks-testing-first and reuse-strict pass. _2026-10-15 GREEN: `make test-gates` 131/131; tasks-testing-first and reuse-strict ok in `make gate`._

## 2. Software Center versioned installs

- [x] 2.1 Migration `V004__versioned_installs.sql` (V003 was already taken by the platforms backfill): rebuild `software_components` with `PRIMARY KEY (id, version)` (copy + swap, `COALESCE(version,'')`), applied through the module migration runner. _V004__versioned_installs.sql applied through the module runner; software_center suites green._
- [x] 2.2 Thread `version: Option<String>` through `preview_install` / `install_artifact` / `start_artifact_install` / uninstall + ensure-writer; add `SoftwareCenterError::UnknownVersion`; update `store.rs` installed-read, update-badge SQL, `StorefrontEntry` install summary. _version threaded + UnknownVersion exercised by 1.1 evidence._
- [x] 2.3 Surface in api DTOs (optional `version` field), web storefront install controls, and CLI `software install --version`; keep payloads backwards compatible. _api DTO / web storefront / CLI surfaces covered by 1.1 integration evidence; gate green._

## 3. Live ACME HTTP-01 wiring

- [x] 3.1 Extend `AcmeHttpServer` with `attach_token_lookup` hook; router falls back to hook after manual map; detach on completion. _attach_token_lookup covered by ssl hook tests (1.2)._
- [x] 3.2 Implement `RustlsAcmeClient::issue`: `AcmeOrderDriver` seam + rustls-acme 0.13 `AcmeState` event loop, in-memory capture `Cache`, HTTP-01 via hook, bounded polling with `backoff_for_attempt`, error classification through `classify_acme_error`/`map_issuance_error`; remove stub + `unreachable!` path. _RustlsAcmeClient::issue driver covered by acme_mock (1.2)._
- [x] 3.3 Verify staging-default / production-opt-in gating untouched (`default_safe`, CLI handler selection) and renewal task still drives the same trait. _gating/renewal verified: ssl suite green (1.2), clippy/docs gates green._

## 4. WebDAV bounded context

- [x] 4.1 `domain/src/webdav/`: `WebDavAccount` aggregate + VOs (reuse `ftp::FtpPath`), error enum, repository trait; register in `lib.rs`. _domain context registered; 5 unit tests green._
- [x] 4.2 `app/src/webdav/`: migration `V001__init.sql` (`webdav_accounts`), repo impl, `WebDavService` (CRUD, argon2 auth mirroring `FtpAuthenticator`), `module.rs` registered in the composition registry. _migration + WebDavService + module registered; 16 router/service tests green._
- [x] 4.3 `app/src/webdav/dav.rs`: axum router with verb handlers (OPTIONS/PROPFIND/GET/HEAD/PUT/DELETE/MKCOL/MOVE/COPY/PROPPATCH; LOCK/UNLOCK 501), Basic auth, chroot path resolver, read-only enforcement. _verb handlers + chroot + read-only verified by tests/webdav.rs._
- [x] 4.4 Adapters: `api/src/routes/webdav.rs` account CRUD + sessions-free, `web/src/webdav.rs` site page, CLI `webdav` subcommands, `/dav` mount in composition root; audit + i18n keys. _api/web/CLI adapters + /dav mount verified by integration webdav tests._

## 5. Staging mail capture

- [x] 5.1 `domain/src/mail_capture/`: `CapturedMessage`, `MailCapturePort` trait; site_staging slot gains `mail_capture: bool` (default false) in domain + staging migration column. _slot toggle default-off + column verified by site_staging suite._
- [x] 5.2 `app/src/mail_capture/`: migration `V001__init.sql`, SQLite service (store with AES-256-GCM body, FIFO cap, purge, list/read views), module registration. _SQLite service (AES-256-GCM, FIFO cap, purge) verified by tests/mail_capture.rs._
- [x] 5.3 Hook `webmail_client::send`: divert when slot capture on (return captured success, no SMTP), unchanged otherwise; slot destroy purges inbox. _divert-vs-passthrough + destroy-purges verified (8 passed)._
- [x] 5.4 Surfaces: web capture inbox (list/detail/purge) under mail views, api endpoints, CLI toggle; audit events; bodies never in audit/logs. _inbox surfaces + audit verified by integration mail_capture tests._

## 6. Verification & docs

- [x] 6.1 Full `make gate` green (fmt, clippy, tests incl. new files, layering, reuse-strict, spec-drift, coverage-floor, governance-contract). _2026-10-15: `make gate` exit 0, every step PASS (incl. governance-quality after .project.json discoverability/publication declaration and script stdout fix); `make check` all steps ok._
- [x] 6.2 Update `docs/CAPABILITIES.md`, `docs/competitive-gap-analysis.md` (WebDAV + capture now specified), `README.md` feature list, `ROADMAP.md` strikethroughs; `HANDOFF.md` follow-up recorded as the second commit per cadence. _docs/CAPABILITIES.md regenerated; competitive-gap-analysis item 9 + batch note; README feature list; ROADMAP out-of-scope line updated; HANDOFF follow-up lands as commit 2._
