# Add TDD Infrastructure

## Why

OpenPanel has accumulated 36 unit tests across 4 implementation changes,
but the test layer has no shared conventions, no repeatable isolation,
no property-based coverage, and no integration tests that exercise the
HTTP stack end-to-end. This makes it easy to write tests that pass
against the current implementation while hiding real bugs — exactly
what the project standard (see `Agents.md`) forbids:

> Tests exist to verify correctness, not to accommodate the code so it
> passes checks; tests act as overseers of the code, not its allies.

This change establishes a **repeatable TDD infrastructure** with:

- Per-test isolation (every test gets a fresh DB; no shared mutable
  state; no assumption about prior runs).
- A fixture library that seeds only what each test needs.
- Mocking at ports (repository traits, audit service, time, randomness)
  via `mockall`.
- Property-based tests via `proptest` for domain invariants.
- HTTP mocking for outbound calls via `wiremock`.
- Optional Docker fixtures via `testcontainers` (for real MySQL/Postgres
  in integration suites).
- An integration test harness that boots the real axum server against
  an isolated SQLite file and exercises it via `reqwest`.

Every future change MUST include a "Testing" task group as the first
group in `tasks.md`, with cases listed in the format defined in this
spec.

## What Changes

- New `tests/` workspace directory at the repo root with:
  - `tests/common/` — shared test helpers (DB helpers, fixtures,
    mock builders)
  - `tests/integration/` — full-stack HTTP integration tests
  - `tests/properties/` — property-based tests for domain invariants
  - `tests/cli/` — end-to-end CLI tests invoking the `openpanel` binary
- New `openpanel-test-support` workspace crate providing:
  - `TestDb` — RAII wrapper around a per-test SQLite file, auto-removed
  - `TestServer` — boots the real axum router with `TestDb`
  - `fixtures::user()`, `fixtures::site()`, `fixtures::database()` —
    factory builders (no global state, no `OnceCell`)
  - `MockAudit`, `MockSiteRepository`, `MockUserRepository` —
    `mockall`-based mocks at port boundaries
  - `Property` helpers for domain round-trips
- New per-crate `#[cfg(test)]` modules adopting:
  - `mockall` for trait mocks (replacing hand-rolled fake impls)
  - `proptest` for domain invariant tests
- New documentation in `Agents.md` (already present) cross-referencing
  this spec as the source of testing conventions.
- New CI script `scripts/check-tests.sh` (informational — no CI server
  yet) that runs `cargo test --workspace`, `cargo fmt --check`,
  `cargo clippy --workspace -- -D warnings`, and `cargo audit`.

## Capabilities

### New Capabilities

- `test-conventions` — where tests live, how they are named, which
  categories exist (unit / integration / property / e2e).
- `test-isolation` — every test owns its DB and its port-mocks; no
  shared mutable state; `cargo test` runs in any order without
  flakiness.
- `test-fixtures` — `TestDb`, `TestServer`, `MockAudit`,
  `MockSiteRepository`, etc. Live in `openpanel-test-support` and
  consumed via `use openpanel_test_support::*;`.
- `property-testing` — `proptest` round-trips for domain invariants
  (e.g. `Site::new` rejects all bad domains; `Password::hash` →
  `verify` is bijective).
- `mocking` — `mockall` for ports; `wiremock` for any outbound HTTP
  (currently none in v0.1; framework in place for future).
- `integration-testing` — full-stack HTTP tests via `reqwest` against
  a booted `axum::Router` with a per-test DB.

### Modified Capabilities

- `architecture` — adds the test-support crate; documents the
  test layer in the layered diagram.

## Impact

- **New crate**: `crates/openpanel-test-support/` — test helpers
  depended on by `dev-dependencies` of every crate that needs it.
- **New top-level**: `tests/` workspace directory with `common`,
  `integration`, `properties`, `cli` subdirs.
- **Crate updates**: each crate gains `dev-dependencies` for
  `openpanel-test-support`, `mockall`, `proptest`.
- **Workspace deps**: `mockall = "0.13"`, `proptest = "1"`,
  `wiremock = "0.6"`, `testcontainers = "0.23"` (optional).
- **Agents.md** — updated to reference this spec.
- **scripts/check-tests.sh** — new shell script; informational.
- **Each future `tasks.md`** — `## 1. Testing` group becomes mandatory.