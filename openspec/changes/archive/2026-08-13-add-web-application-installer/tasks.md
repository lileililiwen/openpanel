# Add web application installer — Tasks

## 1. Testing

- [x] 1.1 Unit tests for idempotency, plan diff, secret rotation.
- [x] 1.2 Property tests: plan content_hash stable; full rollback
      restores every byte (1000 cases).
- [x] 1.3 Service tests with mock catalog and DB.
- [x] 1.4 Integration: live install of hello-world fixture.
- [x] 1.5 CLI E2E: full lifecycle.
- [x] 1.6 Web: install dialog (CSRF), upgrade confirmation, list.

## 2. Domain and Application

- [x] 2.1 Implement `InstallPlan`, `InstallRun`,
      `InstalledWebApp`, `IdempotencyKey`, `SecretCiphertext`
      under
      `crates/openpanel-domain/src/web_application_installer/`.
- [x] 2.2 Add SQLite migrations for `web_app_installs`,
      `web_app_runs`, `web_app_idempotency`.
- [x] 2.3 Implement `WebApplicationInstallerService` and
      register the module.

## 3. Adapters and UI

- [x] 3.1 Add `/api/v1/web-apps/*` routes.
- [x] 3.2 Add `openpanel webapp {install,upgrade,uninstall,list}`.
- [x] 3.3 Build `/web-apps` install page with CSRF.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [ ] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: install WordPress-style fixture on a test
      site, verify DB connection, upgrade and uninstall.
- [x] 4.4 Archive with `openspec archive 2026-08-13-add-web-application-installer`.
