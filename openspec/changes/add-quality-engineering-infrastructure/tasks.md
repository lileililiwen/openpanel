# Tasks: Add Quality Engineering Infrastructure

> **Standing rule (from `Agents.md`):**
> Implementation tasks MUST NOT be marked complete until the test
> tasks in `## 1. Testing` (defined in `add-tdd-infrastructure`)
> pass. Quality tasks in this change are the same — `## 1. Testing`
> below adds the regression tests that the new lints catch.

## 1. Testing — Lint Regression Coverage

- [ ] 1.1 Add unit test in `openpanel-test-support` that asserts a
      fixture file with `.unwrap()` is caught by clippy when the
      test crate is built with `--all-targets`. This is the
      canary for the policy below.
- [ ] 1.2 Add integration test in `tests/integration/quality.rs`
      that runs `scripts/check-quality.sh` against a fresh fixture
      workspace and asserts exit code 0; the same script against a
      fixture that contains `unwrap()` must exit non-zero.
- [ ] 1.3 Add a `proptest` arb that produces a `String`-containing
      arbitrary bytes; assert `Path::new` returns `Err` for any
      input with null bytes (regression: previously this branch was
      not covered by exhaustive cases).

## 2. Lint Policy Configuration

- [ ] 2.1 Add root `clippy.toml` with:
      - `unwrap_used = "deny"`
      - `expect_used = "deny"`
      - `panic_used = "deny"`
      - `todo = "deny"`
      - `unimplemented = "deny"`
      - `disallowed-methods` block listing `std::panic::catch_unwind`
        (informational) and any panic-prone methods added later
- [ ] 2.2 Add `[workspace.lints.rust]` to root `Cargo.toml`:
      - `unsafe_code = "forbid"`
      - `missing_docs = "warn"`
- [ ] 2.3 Add `#![deny(rustdoc::broken_intra_doc_links)]` to the
      lib root of every crate

## 3. Production Code Cleanup

- [ ] 3.1 Remove every `unwrap()` in production code
      (`openpanel-core`, `openpanel-domain`, `openpanel-app`,
      `openpanel-api`, `openpanel-cli`, `openpanel-agent`); replace
      with `?` propagation, `match`, or `.expect("invariant: ...")`
- [ ] 3.2 Remove every `.expect(` in production code unless the
      message is a meaningful invariant description
- [ ] 3.3 Remove every `panic!`, `todo!`, `unimplemented!` in
      production code; replace with proper error types
- [ ] 3.4 Remove every `unsafe` block in production code
      (none expected; this is a defensive cleanup)

## 4. Format Policy

- [ ] 4.1 Run `cargo fmt --all` once across the workspace to
      normalise formatting
- [ ] 4.2 Add `rustfmt.toml` at repo root pinning `edition = "2024"`,
      `max_width = 100`, and stable features
- [ ] 4.3 Verify `cargo fmt --all -- --check` passes after the
      initial format pass

## 5. Dependency Audit

- [ ] 5.1 Add `cargo-audit` to the workspace as a dev-dependency
      (already cached at `0.22.2`)
- [ ] 5.2 Create `audit-suppressions.toml` with the `advisory-db`
      source URL and any known false positives
- [ ] 5.3 Verify `cargo audit` exits 0 on the current dep graph

## 6. Documentation Link Check

- [ ] 6.1 Add `#![deny(rustdoc::broken_intra_doc_links)]` at the lib
      root of every crate (already in task 2.3)
- [ ] 6.2 Run `cargo doc --workspace --no-deps` and fix any broken
      links in existing doc comments
- [ ] 6.3 Add `cargo doc --workspace --no-deps` to
      `scripts/check-quality.sh`

## 7. Quality Script

- [ ] 7.1 Create `scripts/check-quality.sh`:
      - shebang `#!/usr/bin/env bash`, `set -euo pipefail`
      - step 1: `cargo fmt --all -- --check`
      - step 2: `cargo clippy --workspace --all-targets -- -D warnings`
      - step 3: `cargo doc --workspace --no-deps`
      - step 4: `cargo audit`
      - step 5: source `scripts/check-tests.sh` (from the TDD change)
      - Each step prints `step: <name> status: ok | failed`
- [ ] 7.2 Make the script executable (`chmod +x`)
- [ ] 7.3 Verify `scripts/check-quality.sh` exits 0 on the
      current codebase

## 8. CI Workflow

- [ ] 8.1 Create `.github/workflows/ci.yml`:
      - `name: ci`
      - `on: [push, pull_request]`
      - jobs.test: ubuntu-latest, install Rust stable,
        cache cargo registry + target, run `scripts/check-quality.sh`
      - jobs.coverage: ubuntu-latest, run `scripts/coverage.sh`,
        upload `target/coverage/lcov.info` as artifact (informational)
- [ ] 8.2 Verify the YAML is valid (`python3 -c 'import yaml;
      yaml.safe_load(open(".github/workflows/ci.yml"))'`)

## 9. Coverage (Informational)

- [ ] 9.1 Add `cargo-llvm-cov` to the offline-available list (or
      document a fallback to `cargo-tarpaulin`); for v0.1, use
      `cargo-tarpaulin` if available, otherwise emit a stub lcov
      report that says "coverage: unknown"
- [ ] 9.2 Create `scripts/coverage.sh` that runs the chosen tool
      and emits `target/coverage/lcov.info` + a single stdout line
      `coverage: <percent>|unknown`
- [ ] 9.3 Add the script to `.github/workflows/ci.yml` as the
      `coverage` job (informational; does not fail the build)

## 10. Documentation

- [ ] 10.1 Update root `README.md` with a "Quality" section that
      describes the lint policy, the format policy, the audit gate,
      and how to run `scripts/check-quality.sh` locally
- [ ] 10.2 Update `Agents.md` with a "Quality Engineering" section
      that explains why each rule exists and how to fix violations

## 11. Validation

- [ ] 11.1 `cargo clippy --workspace --all-targets -- -D warnings`
      exits 0
- [ ] 11.2 `cargo fmt --all -- --check` exits 0
- [ ] 11.3 `cargo audit` exits 0
- [ ] 11.4 `cargo doc --workspace --no-deps` exits 0
- [ ] 11.5 `scripts/check-quality.sh` exits 0
- [ ] 11.6 Adding a `.unwrap()` to production code causes the
      clippy step to fail (regression test)
- [ ] 11.7 Commit + archive via OpenSpec