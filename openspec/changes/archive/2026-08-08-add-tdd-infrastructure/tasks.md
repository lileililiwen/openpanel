# Tasks: Add TDD Infrastructure

> **Standing rule (see spec requirement `Tests Are Written Before Code`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

## 1. Testing — Test Support Crate

- [x] 1.1 `crates/openpanel-test-support/Cargo.toml` declares deps:
      `tokio`, `axum`, `reqwest`, `serde_json`, `mockall`, `uuid`,
      `tempfile`, and workspace deps for `openpanel-core`,
      `openpanel-domain`, `openpanel-app`, `openpanel-api`.
- [x] 1.2 `TestDb` in
      `crates/openpanel-test-support/src/db.rs`: per-test SQLite file
      under `/tmp/openpanel-test/<uuid>.db`, runs all migrations,
      `Drop` removes the file. Test
      `db::tests::testdb_isolates_per_test` asserts two calls produce
      distinct files and closing one does not affect the other.
- [x] 1.3 `TestServer` in
      `crates/openpanel-test-support/src/server.rs`: wires identity +
      sites + databases + files modules from the production
      `openpanel_app`, boots `axum::serve` on a random port, `Drop`
      aborts the server task. Test
      `server::tests::testserver_boots_and_health` asserts `GET /health`
      returns `{"status":"ok"}` and a second `TestServer` works
      concurrently.
- [x] 1.4 Mocks in
      `crates/openpanel-test-support/src/mocks.rs`: `MockAudit`,
      `MockUserRepo`, `MockSessionRepo`, `MockSiteRepo`,
      `MockDatabaseRepo` via `mockall::mock!`. Test
      `mocks::tests::mock_audit_records_called_with_matching_event`
      sets `expect_record().times(1)`, exercises the mock, and proves
      mockall enforces expectations on Drop.

## 2. Testing — Per-Crate Adoption

- [x] 2.1 `openpanel-test-support` declared in `dev-dependencies` of
      `openpanel-domain`, `openpanel-app`, `openpanel-api`,
      `openpanel-cli`.
- [x] 2.2 `mockall` declared in `dev-dependencies` of `openpanel-app`.
- [x] 2.3 `proptest` declared in `dev-dependencies` of
      `openpanel-domain` and `openpanel-app`.
- [x] 2.4 Identity login unit test added in
      `crates/openpanel-app/src/identity/service.rs::tests`
      using `MockUserRepo` + `MockSessionRepo` + `MockAudit` to assert
      `login()` returns `AccountDisabled` when the user is disabled.
- [x] 2.5 `Site::new` invariants — kept; proptest sibling added in
      `openpanel-domain/src/sites/site.rs::prop` (alias matching
      primary, document root traversal, domain with control chars /
      slash / space / wildcard).
- [x] 2.6 Databases crypto roundtrip — kept in
      `openpanel-app/src/databases/crypto.rs::tests`. Proptest siblings
      live in the domain database module (`prop_auto_prefixed_name`,
      `prop_invalid_owner_username_rejected`).
- [x] 2.7 `Path::new` rejects bad inputs — kept in
      `openpanel-domain/src/files/path.rs::tests`. Proptest siblings
      added in `path.rs::prop` (`prop_path_classifier_matches_validator`,
      `prop_null_bytes_rejected`, `prop_root_is_always_valid`,
      `prop_empty_string_is_root`).

## 3. Testing — Property Tests

- [x] 3.1 `proptest!` block in
      `openpanel-domain/src/sites/site.rs::prop` covering alias
      matching primary rejection, document-root traversal, domain
      control chars / slash / space / wildcard.
- [x] 3.2 `proptest!` block in
      `openpanel-domain/src/common/password.rs::prop`:
      `Password::hash(p).verify(p) == true` for `len(p) >= 12`;
      rejects shorter.
- [x] 3.3 `proptest!` block in
      `openpanel-domain/src/databases/database.rs::prop`:
      auto-prefixed `name = {owner_username}_{suffix}` for any valid
      owner + suffix; rejects owner usernames that don't match
      `[a-z0-9_]{3,32}`.
- [x] 3.4 `proptest!` block in
      `openpanel-domain/src/files/path.rs::prop`:
      `Path::new(p) == Err` iff `p` is absolute / contains `..` /
      contains a null byte; `Path::root()` always succeeds.

## 4. Testing — Integration Tests (HTTP)

- [x] 4.1 `tests/common/mod.rs` re-exports
      `openpanel_test_support::*`.
- [x] 4.2 `tests/integration/identity.rs` covers login success, bad
      password, unknown user, disabled user, `/me` with token, `/me`
      without token, admin create user 200 + admin create user 403.
      Every test starts with a fresh `TestServer`.
- [x] 4.3 `tests/integration/sites.rs`: empty list, 404 on bad uuid,
      insert via service then list.
- [x] 4.4 `tests/integration/databases.rs`: empty list, full
      create/change-password/delete roundtrip when `mysql` CLI is
      present; skipped otherwise.
- [x] 4.5 `tests/integration/files.rs`: PUT/GET/PATCH-rename/DELETE
      roundtrip with a sandboxed document root; chroot escape rejected
      (400 on `..%2F..%2Fetc%2Fpasswd`).
- [x] 4.6 `tests/integration/smoke.rs`: boots `TestServer`, asserts
      `/health` 200 and every route under `/api/v1` returns non-500
      even without auth.

## 5. Testing — E2E CLI Tests

- [x] 5.1 `tests/cli/common.rs` provides `CliRunner` spawning
      `env!("CARGO_BIN_EXE_openpanel")` with
      `OPENPANEL__DATABASE__URL=<testdb>`, returning stdout/stderr/exit.
- [x] 5.2 `tests/cli/serve.rs` spawns `openpanel serve`, hits `/health`,
      kills the server, asserts exit.
- [x] 5.3 `tests/cli/user.rs` creates admin + lists users.
- [x] 5.4 `tests/cli/site.rs` (nginx-gated).
- [x] 5.5 `tests/cli/database.rs` (mysql-gated).
- [x] 5.6 `tests/cli/file.rs` write/read roundtrip.

## 6. Testing — Scripts and Hooks

- [x] 6.1 `scripts/check-tests.sh` runs `cargo check --workspace
      --all-targets` then `cargo test --workspace --all-targets
      -- --test-threads=1`. `fmt` / `clippy` / `docs` / `audit` are
      dispatched by the root `Makefile` (single entry point: `make
      check`).
- [x] 6.2 `scripts/install-hooks.sh` installs a pre-commit hook that
      runs `make check`. Informational: refuses to clobber an existing
      hook and prints the manual merge instructions instead. `git
      commit --no-verify` bypasses.
- [x] 6.3 Root `README.md` "Testing" section cross-references
      `crates/openpanel-test-support/README.md` and `tests/README.md`.

## 7. Documentation

- [x] 7.1 `crates/openpanel-test-support/README.md` describes the four
      test categories, the `TestDb` / `TestServer` / `Mock*`
      fixtures, sandbox-path usage, and a quick-start example.
- [x] 7.2 `Agents.md` § 4 "Test Discipline (TDD Infrastructure)"
      references this spec as the source of testing conventions and
      enforces the `## 1. Testing`-first rule.
- [x] 7.3 `tests/README.md` explains how to run unit / property /
      integration / CLI E2E tests vs the conventions for adding a new
      integration test.

## 8. Validation

- [x] 8.1 `cargo test --workspace` passes (83 tests across all
      crates).
- [x] 8.2 `cargo clippy --workspace --all-targets -- -D warnings`
      passes (with `#![cfg_attr(test, allow(...))]` for test code;
      deny in production code via the
      `add-quality-engineering-infrastructure` lints).
- [x] 8.3 `cargo fmt --all -- --check` passes.
- [x] 8.4 `make check` exits 0.
- [x] 8.5 Two consecutive `cargo test --workspace` runs produce
      identical test counts (83) with zero failures / flakes.
- [x] 8.6 Commit + archive via OpenSpec.

## Notes

- The test-support crate is `dev-dependencies` only — it MUST NOT
  appear in `[dependencies]` of any production crate.
- `mockall`'s `mock!` macro generates a struct that lives next to
  the trait; place mocks in `openpanel-test-support/src/mocks.rs`
  grouped by trait.
- `proptest` failures should be reproducible: when a failing case is
  found, proptest prints the seed; capture it in a `// proptest-seed:`
  comment so the test can be re-run with the same input.