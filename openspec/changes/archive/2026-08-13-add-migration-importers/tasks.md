# Add migration importers — Tasks

## 1. Testing

- [x] 1.1 Unit tests for the reference driver's `sniff` and
      manifest parser.
- [x] 1.2 Property tests: idempotency guard refuses a re-run of
      an already-imported plan.
- [x] 1.3 Service tests with mock translators (preview / run /
      rollback and translation log correctness).
- [x] 1.4 Integration: live preview / run / rollback through the
      real SQLite-backed service.
- [ ] 1.5 CLI E2E: full lifecycle. (deferred)
- [ ] 1.6 Web: import wizard (CSRF), progress, rollback. (deferred)

## 2. Domain and Application

- [x] 2.1 Implement `MigrationDriver`, `MigrationPlan`,
      `ImportedResource`, `TranslationLog`, `MigrationRun`,
      `MigrationError` under
      `crates/openpanel-domain/src/migration_importers/`.
- [x] 2.2 Add SQLite migration for `migration_runs`,
      `imported_resources`, `translation_log_entries`.
- [x] 2.3 Implement `MigrationService`, the SQLite repository,
      and the reference `tar-with-json-manifest` driver.
- [ ] 2.4 Implement `cpanel-pkgacct`, `cpanel-legacy-backup`, and
      `baota-backup` native-tar drivers. (deferred)

## 3. Adapters and UI

- [ ] 3.1 Add `/migration/import/*` REST routes. (deferred)
- [ ] 3.2 Add `openpanel migration {preview,run,rollback,imports}`. (deferred)
- [ ] 3.3 Build the import wizard with progress and rollback. (deferred)

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc nits).
- [x] 4.3 Smoke-test: a tar-with-json-manifest bundle imports to
      sites; rollback undo works within 24h.
- [x] 4.4 Archive with `openspec archive add-migration-importers`.

## 5. Module Wiring

- [x] 5.1 Register `MigrationImportersModule` in
      `crates/openpanel-app/src/lib.rs`.
- [x] 5.2 Wire the module into `crates/openpanel-cli/src/handlers.rs`
      and `crates/openpanel-test-support/src/server.rs`.
- [x] 5.3 Add `MigrationPreviewed`, `MigrationRunCommitted`,
      `MigrationRunRolledBack`, `MigrationAlreadyImportedRejected`
      audit actions.