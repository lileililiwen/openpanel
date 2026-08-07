# Tasks: Add TDD Infrastructure

> **Standing rule (see spec requirement `Tests Are Written Before Code`):**
> The first task group below MUST be `## 1. Testing`. Implementation
> tasks in groups `## 2.`, `## 3.`, etc. MUST NOT be marked complete
> until the tests in `## 1.` are green.

## 1. Testing — Test Support Crate

- [ ] 1.1 Create `crates/openpanel-test-support/Cargo.toml` with
      deps: `tokio`, `axum`, `reqwest`, `serde_json`, `mockall`,
      `uuid`, `tempfile` (optional), workspace deps for
      `openpanel-core` / `openpanel-domain` / `openpanel-app` /
      `openpanel-api`
- [ ] 1.2 Implement `TestDb` in
      `crates/openpanel-test-support/src/db.rs`:
      - `new() -> Self` — creates `/tmp/openpanel-test/<uuid>.db`,
        opens pool, runs all migrations from every registered module
      - `pool()`, `url()` accessors
      - `Drop` removes the file
      - **Test case**: `tests_testdb_isolates_per_test` — two
        `TestDb::new()` calls produce different files; closing
        one does not affect the other
- [ ] 1.3 Implement `TestServer` in
      `crates/openpanel-test-support/src/server.rs`:
      - `new(db: TestDb) -> Self` — wires identity + sites +
        databases + files modules from the production `openpanel_app`,
        boots `axum::serve(listener, router)` on a random port
      - `addr() -> String`, `client() -> reqwest::Client` accessors
      - `Drop` aborts the server task
      - **Test case**: `tests_testserver_boots_and_health` —
        `GET /health` returns `{"status":"ok"}`; `GET /health` on a
        second `TestServer` works concurrently
- [ ] 1.4 Implement mocks in
      `crates/openpanel-test-support/src/mocks.rs`:
      - `MockUserRepository`, `MockSessionRepository`,
        `MockSiteRepository`, `MockDatabaseRepository`,
        `MockFileRepository`, `MockAudit` — all via `mockall::mock!`
      - **Test case**: `tests_mock_audit_records_called_with_matching_event`
        — set `expect_record(1, predicate)` matching `action ==
        AuditAction::Login`; call service; `verify()` passes;
        calling service twice fails verification

## 2. Testing — Per-Crate Adoption

- [ ] 2.1 Add `openpanel-test-support = ...` to `dev-dependencies`
      of `openpanel-domain`, `openpanel-app`, `openpanel-api`,
      `openpanel-cli`
- [ ] 2.2 Add `mockall` to `dev-dependencies` of `openpanel-app`
- [ ] 2.3 Add `proptest` to `dev-dependencies` of `openpanel-domain`
      and `openpanel-app`
- [ ] 2.4 Migrate the identity unit test (`login` returns
      `AccountDisabled` when user disabled) to use `MockUserRepository`
      — drop the existing in-memory SQLite setup
- [ ] 2.5 Migrate the sites unit test (`Site::new` invariants) — keep
      as-is (already pure domain); add proptest sibling
- [ ] 2.6 Migrate the databases unit test (crypto roundtrip,
      tampered ciphertext) — keep; add proptest sibling for
      arbitrary plaintexts
- [ ] 2.7 Migrate the files unit test (`Path::new` rejects bad inputs)
      — keep; add proptest sibling with `Arbitrary` impl for
      `String`-shaped inputs

## 3. Testing — Property Tests

- [ ] 3.1 Add `proptest!` block to `openpanel-domain/src/identity/site.rs`
      asserting `Site::new(anything)`:
      - rejects names containing `/`, spaces, `*`, control chars
      - rejects document roots not under `/var/www/`
      - rejects aliases matching the primary domain
      - 100 cases by default; `PROPTEST_CASES=1000 cargo test` for CI
- [ ] 3.2 Add `proptest!` block to
      `openpanel-domain/src/identity/password.rs`:
      - `Password::hash(p).unwrap().verify(p).unwrap() == true` for
        any `p: String` ≥ 12 chars
      - `Password::hash(p)` returns `Err(TooShort)` for `len(p) < 12`
- [ ] 3.3 Add `proptest!` block to
      `openpanel-domain/src/databases/database.rs`:
      - auto-prefixed `name = {owner_username}_{suffix}` for any
        valid `owner_username` and `suffix`
      - rejects owner usernames that don't match `[a-z0-9_]{3,32}`
- [ ] 3.4 Add `proptest!` block to
      `openpanel-domain/src/files/path.rs`:
      - `Path::new(p)` returns `Err` iff `p` is absolute OR contains
        `..` OR contains a null byte
      - `Path::root()` always succeeds

## 4. Testing — Integration Tests (HTTP)

- [ ] 4.1 Create `tests/common/mod.rs` re-exporting
      `openpanel_test_support::*`
- [ ] 4.2 Create `tests/integration/identity.rs` covering:
      - login success → 200 + token + session cookie set
      - login with bad password → 401 + `invalid_credentials`
      - login with disabled user → 401 + `account_disabled`
      - GET `/me` with valid token → 200 + user DTO
      - GET `/me` without token → 401 + `unauthorized`
      - Admin trying to create a user → 403 + `forbidden`
      - Each test MUST start with a fresh `TestDb` and `TestServer`
- [ ] 4.3 Create `tests/integration/sites.rs` covering:
      - List sites (empty array) for fresh DB
      - 404 on `/sites/<bad-uuid>`
      - Insert a site directly via `TestDb` + repo; list returns it
- [ ] 4.4 Create `tests/integration/databases.rs` covering:
      - Detect `mysql` CLI; skip the test with `eprintln!` if absent
      - When `mysql` is present: full create / list / change-password
        / delete round-trip with assertions against the shell-out
- [ ] 4.5 Create `tests/integration/files.rs` covering:
      - Use a per-test document root under `/tmp/openpanel-test/<uuid>/`
      - Insert a `Site` row via the repo pointing at that root
      - PUT a file, GET it back, PATCH rename, DELETE it
      - Assert chroot: `..%2F..%2Fetc%2Fpasswd` returns 400
- [ ] 4.6 Create `tests/integration/smoke.rs` that boots `TestServer`
      and walks every route once (no DB rows assumed) — this is the
      "does the server even start?" gate

## 5. Testing — E2E CLI Tests

- [ ] 5.1 Create `tests/cli/common.rs` with `CliRunner` helper that
      spawns `env!("CARGO_BIN_EXE_openpanel")` with
      `OPENPANEL__DATABASE__URL=<testdb>` and returns
      `{stdout, stderr, exit_code}`
- [ ] 5.2 `tests/cli/serve.rs`: spawn `openpanel serve`, hit
      `/health` via curl against the server's port, kill the server,
      assert exit code on shutdown
- [ ] 5.3 `tests/cli/user.rs`: create admin → list users → assert
      the list contains the admin
- [ ] 5.4 `tests/cli/site.rs`: skipped if no nginx; otherwise create
      a site → list → assert
- [ ] 5.5 `tests/cli/database.rs`: skipped if no mysql; otherwise
      create database → change-password → delete
- [ ] 5.6 `tests/cli/file.rs`: write a file via the CLI → read it
      via the CLI → assert byte-equal content

## 6. Testing — Scripts and Hooks

- [ ] 6.1 Create `scripts/check-tests.sh` that runs:
      - `cargo fmt --all -- --check`
      - `cargo clippy --workspace --all-targets -- -D warnings`
      - `cargo test --workspace`
      - `cargo audit`
      - Exits non-zero on any failure
- [ ] 6.2 Add `.git/hooks/pre-commit` (or document in
      `scripts/install-hooks.sh`) that runs `check-tests.sh` —
      informational; no enforcement in v0.1
- [ ] 6.3 Update root `README.md` with a "Testing" section that
      cross-references `crates/openpanel-test-support/README.md`

## 7. Documentation

- [ ] 7.1 Add `crates/openpanel-test-support/README.md` describing
      the four test categories, the fixture API, and a quick start
- [ ] 7.2 Update root `Agents.md` to reference this spec as the
      source of testing conventions (already drafted)
- [ ] 7.3 Add `tests/README.md` explaining how to run integration /
      property / e2e tests vs unit tests

## 8. Validation

- [ ] 8.1 `cargo test --workspace` passes — all existing 36 tests
      plus new ones
- [ ] 8.2 `cargo clippy --workspace --all-targets -- -D warnings`
      passes (with allow attributes for `unwrap`/`expect` in test
      code; deny in production code; see
      `add-quality-engineering-infrastructure`)
- [ ] 8.3 `cargo fmt --all -- --check` passes
- [ ] 8.4 `scripts/check-tests.sh` exits 0
- [ ] 8.5 Running `cargo test` twice in a row produces identical
      test counts and no flakes
- [ ] 8.6 Commit + archive via OpenSpec

## Notes

- The test-support crate is `dev-dependencies` only — it MUST NOT
  appear in `[dependencies]` of any production crate.
- `mockall`'s `mock!` macro generates a struct that lives next to
  the trait; place mocks in `openpanel-test-support/src/mocks.rs`
  grouped by trait.
- `proptest` failures should be reproducible: when a failing case is
  found, proptest prints the seed; capture it in a `// proptest-seed:`
  comment so the test can be re-run with the same input.