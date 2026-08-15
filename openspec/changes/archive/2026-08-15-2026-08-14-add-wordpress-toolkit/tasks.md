# Add WordPress toolkit — Tasks

## 1. Testing

- [x] 1.1 Unit: version compare; local CVE feed match; cache-constant
      render; rollback restores DB snapshot + wp_root.
- [x] 1.2 Property: staging/clone target stays inside panel-managed
      space; a failed update is fully reversible.
- [x] 1.3 Service: stage, clone, update (with rollback on failure),
      scan, cache toggle; audit events record names only.
- [x] 1.4 Integration: live WP stage from a running site; a broken
      update rolls back cleanly; scan flags an outdated plugin and a
      known CVE.
- [ ] 1.5 CLI E2E: `openpanel site wp stage` -> `update` -> `scan`.
- [ ] 1.6 Web: WP tab (CSRF), stage/clone buttons, update panel, scan
      report, cache toggle.

## 2. Domain and Application

- [x] 2.1 Implement `WpSite`, `WpUpdateSet`, `WpUpdateResult`,
      `WpSecurityReport`, `WpCacheMode` under
      `crates/openpanel-domain/src/wordpress_toolkit/`.
- [x] 2.2 Add SQLite migration for `wp_sites`, `wp_update_runs`.
- [x] 2.3 Implement `WpToolkitService`, `WpScanner`, `WpUpdater`,
      `WpCacheLayer`; register the module via `ModuleRegistry`.

## 3. Adapters and UI

- [ ] 3.1 Add `/sites/{id}/wordpress/{staging,clone,updates,security,cache}`
      REST routes.
- [ ] 3.2 Add `openpanel site wp {stage,clone,update,scan,cache}`.
- [ ] 3.3 Build the WP tab (CSRF), stage/clone buttons, update panel,
      scan report, cache toggle.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: install WP via web-application-installer, stage
      it, run an update that fails, confirm rollback; run a scan and
      confirm it reports an outdated plugin.
- [x] 4.4 Archive with `openspec archive add-wordpress-toolkit`.
