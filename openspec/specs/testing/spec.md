# testing Specification

## Purpose
TBD - created by archiving change add-tdd-infrastructure. Update Purpose after archive.
## Requirements
### Requirement: Test Categories and File Locations

Tests MUST be organized into four categories, each with a fixed
location and a fixed naming convention:

| Category | Location | Naming |
|---|---|---|
| Unit | `crates/<crate>/src/**/*.rs` (inside `#[cfg(test)] mod tests`) | `tests::test_<unit_under_test>` |
| Integration | `tests/integration/<area>.rs` | `<area>_<behavior>` |
| Property | `crates/<crate>/src/**/*.rs` (inside `#[cfg(test)] mod prop`) or `tests/properties/<area>.rs` | `prop_<invariant>` |
| E2E (CLI) | `tests/cli/<command>.rs` | `cli_<command>_<scenario>` |

A single behavior MUST have at most one test in each category that
covers it, so that "does this work?" can be answered at three
different scopes without duplication.

#### Scenario: Developer writes a new feature

- **WHEN** a developer implements a new `Foo` module in `openpanel-app`
- **THEN** they MUST add at least one unit test in `foo.rs`'s
  `#[cfg(test)] mod tests`, one integration test in
  `tests/integration/foo.rs`, and at least one property-based test
  covering any invariant `Foo` claims to enforce.

### Requirement: Repeatability and DB Isolation

Tests MUST NOT depend on:
- A running MySQL/Postgres daemon
- The current contents of any SQLite file
- Filesystem state created by a previous test
- Environment variables set outside the test

Every integration / e2e test MUST acquire its own `TestDb` via the
test-support crate. `TestDb::new()` MUST create a fresh SQLite file
under `/tmp/openpanel-test/<uuid>.db`, register it with the running
process, and remove the file on drop.

#### Scenario: Run the full test suite twice in a row

- **WHEN** a developer runs `cargo test --workspace` twice in a row
  with no manual cleanup in between
- **THEN** both runs pass with identical counts of tests, and no
  test from the first run influences any test from the second run.

#### Scenario: Test inserts then queries without depending on existing data

- **WHEN** a test creates a user via the IdentityService and then
  queries the database for that user
- **THEN** it finds exactly one user and that user's id matches the
  id returned by the create call — even if the database previously
  contained other users (which it does not, because `TestDb` is fresh).

### Requirement: Fixture Library (`openpanel-test-support`)

A new workspace crate `openpanel-test-support` MUST be created at
`crates/openpanel-test-support/`. It MUST export:

- `TestDb` — `Drop`-runs SQL `DELETE FROM _migrations` (no, that
  loses migrations); instead `Drop` removes the underlying file.
  Methods: `new() -> Self`, `pool() -> Pool<Sqlite>`,
  `url() -> String`.
- `TestServer` — wraps `openpanel_api::build_router` with a `TestDb`
  and returns the bound `SocketAddr` plus a `reqwest::Client`
  preconfigured with the address. Methods: `addr()`, `client()`.
- `MockAudit` — `mockall::mock!`-style struct implementing
  `AuditService` with `expect_record` / `expect_recent` predicates.
- `MockUserRepository`, `MockSessionRepository`, `MockSiteRepository`,
  `MockDatabaseRepository`, `MockFileRepository` — `mockall` mocks
  for each port.

Fixtures MUST be **factories**, not singletons. Two tests calling
`TestDb::new()` MUST get two independent SQLite files; calling
`MockUserRepository::new()` twice MUST give two independent mock
objects.

#### Scenario: Two tests each get their own DB

- **WHEN** test `A` creates a user `alice`, and test `B` (running
  before or after) calls `list_users`
- **THEN** `B` sees an empty list regardless of `A`'s state.

#### Scenario: Test sets an audit expectation that is checked

- **WHEN** a test calls `MockAudit::expect_record(1, predicate)`
- **THEN** after the test calls `IdentityService::login(...)`, the
  mock's `verify()` passes; if the predicate does not match, the
  test fails with a clear diff.

### Requirement: Mocks at Port Boundaries

`mockall` MUST be used to mock every trait in `openpanel-domain`
that lives behind a port (`UserRepository`, `SessionRepository`,
`SiteRepository`, `DatabaseRepository`, `FileRepository`) and
`AuditService`. Tests MUST NOT substitute the concrete SQLite
implementations of those ports when exercising pure business
logic — they MUST use the mocks.

The concrete SQLite implementations (`SqliteUserRepository` etc.) MUST
be exercised only via integration tests that boot a `TestDb`. Unit
tests MUST NOT touch SQLite.

#### Scenario: Unit test for `IdentityService::login` uses `MockUserRepository`

- **WHEN** a unit test verifies that `login` returns
  `IdentityError::AccountDisabled` when the user is disabled
- **THEN** the test uses `MockUserRepository` with
  `expect_find_by_username(...)` returning a user whose `is_disabled()`
  is true. It MUST NOT touch any SQLite file.

### Requirement: Property-Based Tests for Domain Invariants

Every domain aggregate MUST have at least one `proptest` test in
`crates/openpanel-domain/src/<module>.rs` (inside `mod prop`):

- Generates inputs via `proptest!` with `Arbitrary` impls or `proptest::collection`.
- Asserts the invariant directly (e.g. `Site::new` rejects any name
  that doesn't match the RFC 1035 regex).
- Tests at least 100 cases by default; CI may raise this to 1000.

#### Scenario: `Path::new` rejects all `..` inputs

- **WHEN** the proptest generates a `String` containing `..` as a
  component
- **THEN** `Path::new(s)` returns `Err(FileError::InvalidPath)`
  100% of the time, regardless of the surrounding components.

#### Scenario: `Password::hash` roundtrip is bijective

- **WHEN** proptest generates a random plaintext ≥ 12 chars
- **THEN** `Password::hash(p).unwrap().verify(p).unwrap()` returns
  `true` 100% of the time.

### Requirement: Integration Test Harness

`openpanel-test-support::TestServer` MUST boot the real `axum`
router from `openpanel_api::build_router` on a random local port
(using `TcpListener::bind("127.0.0.1:0")`). Integration tests MUST
exercise the server via `reqwest::Client::new()` with the bound
address — no internal-handler shortcuts.

Each integration test MUST:
1. Create a `TestDb` (fresh per test).
2. Build a `TestServer` with the right service modules wired.
3. Log in as a user via `POST /api/v1/identity/login`.
4. Make the call under test with `Authorization: Bearer <token>`.
5. Assert against the response (status, body).
6. Assert against `MockAudit` if the call should have emitted an
   audit event.

#### Scenario: Login then list users

- **WHEN** a test boots `TestServer`, posts `/identity/login` with
  valid credentials, and calls `GET /identity/users`
- **THEN** the response is 200 with a JSON array of one user,
  matching the admin user created in the setup step. The session
  cookie is set. A `login_success` audit row is recorded.

#### Scenario: No shared mutable state between integration tests

- **WHEN** integration tests run in any order (e.g. via
  `cargo nextest` or `cargo test -- --test-threads=4`)
- **THEN** no test fails because of state left by another test.
  Each test starts with a fresh DB, no users, no sessions, no audit
  entries.

### Requirement: E2E CLI Tests

`tests/cli/<command>.rs` MUST spawn the `openpanel` binary as a child
process with `OPENPANEL__DATABASE__URL` pointing at a per-test
`TestDb`, then exercise the binary as the operator would. Output is
parsed line by line; non-zero exit codes MUST fail the test.

#### Scenario: CLI creates a user

- **WHEN** `tests/cli/user.rs::cli_user_create` runs
- **THEN** it executes `./target/debug/openpanel user create ...`,
  asserts exit code 0, asserts stdout contains `created user ...`,
  and asserts the DB row exists via a separate `reqwest`-to-the-
  server call.

### Requirement: Tests Are Written Before Code (Standing Rule)

This requirement is a **standing rule** that applies to every future
OpenSpec change. For every `tasks.md`:

- The first numbered task group MUST be `## 1. Testing`.
- It MUST list at least:
  - one unit test for each new public function
  - one integration test for each new HTTP route
  - one property-based test for each new domain invariant
  - one E2E test for each new CLI subcommand (if applicable)
- Tests MUST be implemented (and failing — TDD) BEFORE the
  implementation tasks in the same change. Implementation tasks
  MUST NOT be marked complete until the corresponding tests pass.

`openspec validate` SHOULD be extended (in a follow-up change) to
enforce this rule.

#### Scenario: New `add-foo` change's tasks.md

- **WHEN** a developer creates a new change `add-foo` and adds three
  new public functions to the application layer
- **THEN** `tasks.md` MUST contain at least three unit-test tasks
  listed under `## 1. Testing` BEFORE any `## 2. Implementation`
  task group. Skipping or reordering MUST be rejected by code review.

### Requirement: Repeatability Across Environments

`cargo test --workspace` MUST pass in any of the following
environments without code changes:

- A fresh container with no MySQL/Postgres installed
- A macOS workstation with Homebrew sqlite
- A Linux host with system sqlite
- The CI runner

If a test requires a tool that is not universally available (e.g.
`mysql`, `nginx`), the test MUST detect the absence and `skip`
rather than fail. The skip MUST be logged so CI can show the test
was skipped, not missing.

#### Scenario: Test that needs `mysql` is skipped when not installed

- **WHEN** `tests/integration/databases.rs` runs on a host without
  `mysql` in PATH
- **THEN** the test prints `skipped: mysql not installed` and the
  test result is reported as `skipped`, not `failed`.

### Requirement: No `unwrap` / `expect` / `panic` in Production Code

This is enforced by a follow-up quality-engineering change via clippy
`unwrap_used`, `expect_used`, `panic_used`, `todo` lints at `deny`
level in non-`#[cfg(test)]` code. This spec asserts the *intent* —
production code MUST NOT contain `unwrap()`, `expect()`, `panic!()`,
`unreachable!()`, `todo!()`, or `unimplemented!()`. Test code MAY
contain them (and SHOULD, when the alternative is verbose).

#### Scenario: Production code uses `?` for error propagation

- **WHEN** a non-test function reads from a repository and the
  repository call returns `Result<_, RepoError>`
- **THEN** the call site uses `?` to propagate, NOT `.unwrap()`.
  Reviewers MUST flag any `unwrap` / `expect` in production code.

### Requirement: `openspec validate` Enforces Tests-First In tasks.md

The standing rule that every `tasks.md` starts with `## 1. Testing`
before any implementation group SHALL be machine-enforced. `make check`
SHALL run `scripts/check-tasks-testing-first.sh`, which parses every
active change under `openspec/changes/*/tasks.md` and fails if any
non-testing top-level group (e.g. `## 2. Implementation`) appears
before a `## 1. Testing` (or equivalent) group. The script prints
`step: tasks-testing-first status: failed` naming the offending change
and exits non-zero. This closes the "SHOULD be extended ... in a
follow-up change" TODO in the original testing spec.

#### Scenario: Testing group missing or reordered

- **WHEN** a change's `tasks.md` lists `## 2. Implementation` before
  `## 1. Testing`
- **THEN** `make check` fails at the `tasks-testing-first` step and the
  change cannot be applied until reordered.

#### Scenario: Correct order passes

- **WHEN** every active change's `tasks.md` has `## 1. Testing` first
- **THEN** the step prints `step: tasks-testing-first status: ok`.

### Requirement: Governance Gate Tests Cover Positive And Negative Paths

Every shell governance gate introduced by the project SHALL have a
fixture-based self-test covering at least one compliant input and one
non-compliant input. The self-test SHALL assert exit status and SHALL use
isolated temporary fixtures so repository state cannot mask a regression.

#### Scenario: Compliant fixture passes

- **WHEN** a fixture satisfies a gate’s documented contract
- **THEN** the self-test asserts exit code zero.

#### Scenario: Non-compliant fixture fails

- **WHEN** a fixture violates a gate’s documented contract
- **THEN** the self-test asserts a non-zero exit code and a useful failure
  diagnostic.

