# Tasks: Add Quality Engineering Infrastructure

> **Standing rule (from `Agents.md`):**
> Implementation tasks MUST NOT be marked complete until the test
> tasks in `## 1. Testing` (defined in `add-tdd-infrastructure`)
> pass. Quality tasks in this change are the same — `## 1. Testing`
> below adds the regression tests that the new lints catch.

## 1. Testing — Lint Regression Coverage

- [x] 1.1 Add unit test in `openpanel-test-support` that asserts a
      fixture file with `.unwrap()` is caught by clippy when the
      test crate is built with `--all-targets`. This is the
      canary for the policy below.
- [x] 1.2 Add integration test in `tests/integration/quality.rs`
      that runs `make check` against a fresh fixture
      workspace and asserts exit code 0; the same script against a
      fixture that contains `unwrap()` must exit non-zero.
- [x] 1.3 Add a `proptest` arb that produces a `String`-containing
      arbitrary bytes; assert `Path::new` returns `Err` for any
      input with null bytes (regression: previously this branch was
      not covered by exhaustive cases).

## 2. Lint Policy Configuration

- [x] 2.1 Root `clippy.toml` carries `disallowed-methods` for
      `std::panic::catch_unwind`. The deny levels
      (`unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented`)
      live in `[workspace.lints.clippy]` of the root `Cargo.toml`,
      which is the correct location for lint levels (clippy.toml
      supports `disallowed-methods` and config keys, not lint
      levels). Each crate inherits via `[lints] workspace = true`.
- [x] 2.2 `[workspace.lints.rust]` sets `unsafe_code = "forbid"`
      and `missing_docs = "warn"`.
- [x] 2.3 `#![deny(rustdoc::broken_intra_doc_links)]` is set at
      the lib root of every crate (openpanel-core, openpanel-domain,
      openpanel-app, openpanel-api, openpanel-cli,
      openpanel-test-support) and at `main.rs` of openpanel-agent.

## 3. Production Code Cleanup

- [x] 3.1 Every `unwrap()` in production code removed. Replaced
      with `?` propagation or `match`. (Remaining production-code
      `expect()`s use meaningful invariant messages and carry
      `#[allow(clippy::expect_used)]` with justification comments.)
- [x] 3.2 Every `.expect(` in production code uses a meaningful
      invariant description (regex/argon2 compile-time constants,
      post-connect pool invariant). Each carries
      `#[allow(clippy::expect_used)]` with a justification comment
      because clippy's `expect_used` lint denies all `expect()`s
      regardless of message.
- [x] 3.3 `panic!` in `openpanel-core` `ModuleRegistry::register`
      replaced with a `Result` return on `CoreError::DuplicateModule`.
      No other production `panic!` / `todo!` / `unimplemented!`
      remain.
- [x] 3.4 No `unsafe` blocks remain in production code. The E2E
      test that previously used `unsafe { libc::kill(...) }` now
      uses `Child::kill()`.

## 4. Format Policy

- [x] 4.1 `cargo fmt --all` run to normalise formatting.
- [x] 4.2 `rustfmt.toml` pins `edition = "2024"`, `max_width = 100`.
- [x] 4.3 `cargo fmt --all -- --check` passes.

## 5. Dependency Audit

- [x] 5.1 `cargo-audit = "0.22"` added to root `[dev-dependencies]`
      and present in `Cargo.lock`.
- [x] 5.2 `.cargo/audit.toml` created with the known
      `RUSTSEC-2023-0071` suppression and justification.
- [x] 5.3 `cargo audit` exits 0. `scripts/check-audit.sh` uses
      `--no-fetch --stale` so the gate works in offline / restricted
      environments using the cached advisory-db.

## 6. Documentation Link Check

- [x] 6.1 Covered by task 2.3.
- [x] 6.2 `cargo doc --workspace --no-deps` passes; every public
      item in every crate now carries a rustdoc comment, so broken
      intra-doc links surface immediately.
- [x] 6.3 `scripts/check-docs.sh` runs `cargo doc --workspace
      --no-deps`; dispatched by `make docs`.

## 7. Quality Gate (Makefile dispatcher)

- [x] 7.1 `scripts/lib/step.sh` exists with the `step` helper.
- [x] 7.2 Per-gate scripts exist:
      `scripts/check-fmt.sh`, `check-clippy.sh`, `check-docs.sh`,
      `check-audit.sh`, `check-tests.sh` — each sources `step.sh`.
- [x] 7.3 Root `Makefile` dispatches `fmt clippy docs audit test`
      from `make check`; each gate is also runnable alone.
- [x] 7.4 All scripts are executable (`chmod +x`).
- [x] 7.5 `make check` exits 0 on the current codebase.

## 8. CI Workflow

- [x] 8.1 `.github/workflows/ci.yml` exists with `name: ci`,
      `on: [push, pull_request]`, and jobs
      `check / fmt / clippy / test / coverage` using the
      `dtolnay/rust-toolchain@stable` action. The `coverage` job
      uploads `target/coverage/lcov.info` as an artifact and is
      marked `continue-on-error: true` (informational).
- [x] 8.2 YAML is valid (verified with `python3 -c 'import yaml;
      yaml.safe_load(...)'`).

## 9. Coverage (Informational)

- [x] 9.1 `scripts/coverage.sh` uses `cargo-llvm-cov` if available,
      otherwise `cargo-tarpaulin`, otherwise emits a stub
      `lcov.info` with `coverage: unknown`.
- [x] 9.2 `scripts/coverage.sh` emits
      `target/coverage/lcov.info` and a single stdout line
      `coverage: <percent>` (or `unknown`).
- [x] 9.3 CI `coverage` job invokes `make coverage` and uploads
      the artifact.

## 10. Documentation

- [x] 10.1 Root `README.md` has a "Quality" section describing the
      lint policy, the format policy, the audit gate, and how to
      run `make check`.
- [x] 10.2 `Agents.md` §5 "Quality Engineering" explains why each
      rule exists and how to fix violations.

## 11. Validation

- [x] 11.1 `cargo clippy --workspace --all-targets -- -D warnings`
      exits 0.
- [x] 11.2 `cargo fmt --all -- --check` exits 0.
- [x] 11.3 `cargo audit` exits 0.
- [x] 11.4 `cargo doc --workspace --no-deps` exits 0.
- [x] 11.5 `make check` exits 0.
- [x] 11.6 Adding a `.unwrap()` to production code causes the
      clippy step to fail. Proved by two regression tests:
      `openpanel-test-support::clippy_canary::clippy_catches_unwrap_in_production_code`
      and `tests::integration::quality::fixture_with_unwrap_fails_clippy`.
- [x] 11.7 Commit + archive via OpenSpec.