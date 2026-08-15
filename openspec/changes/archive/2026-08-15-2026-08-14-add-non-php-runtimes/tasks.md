# Add non-PHP runtimes — Tasks

## 1. Testing

- [x] 1.1 Unit: version-pin validation (allowed list); supervisor unit
      render (user-scoped, chroot cwd); nginx proxy block renders a
      loopback-only `proxy_pass`.
- [x] 1.2 Property: app port can never bind to a non-loopback address;
      runtime working dir cannot escape the site chroot.
- [x] 1.3 Service: set runtime, start, stop, restart, status sync;
      audit `RuntimeChanged` records kind + version only.
- [x] 1.4 Integration: live unit starts the app; nginx proxies a real
      request to `127.0.0.1:APP_PORT`; stopping the unit drops traffic.
- [ ] 1.5 CLI E2E: `openpanel site runtime set node 20` -> `start` ->
      `logs` -> `stop`.
- [ ] 1.6 Web: Runtime tab (CSRF), kind+version selector, control
      buttons, log streaming view.

## 2. Domain and Application

- [x] 2.1 Implement `SiteRuntime`, `RuntimeKind`, `RuntimeVersion`,
      `AppPort`, `RuntimeStatus` under
      `crates/openpanel-domain/src/app_runtimes/`.
- [x] 2.2 Add SQLite migration for `site_runtimes`.
- [x] 2.3 Implement `RuntimeService`, `SupervisorUnitBuilder`,
      `ReverseProxyLayer`; register the module via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/sites/{id}/runtime` and `/sites/{id}/runtime/logs`
      REST routes.
- [ ] 3.2 Add `openpanel site runtime {get,set,start,stop,restart,logs}`.
- [ ] 3.3 Build the Runtime tab (CSRF), selector, control buttons, log
      view.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: set a Node runtime, start it, confirm nginx serves
      the app on the site domain; stop it, confirm 502.
- [x] 4.4 Archive with `openspec archive add-non-php-runtimes`.
