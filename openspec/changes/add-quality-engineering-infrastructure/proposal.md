# Add Quality Engineering Infrastructure

## Why

The TDD infrastructure change (`add-tdd-infrastructure`) gives us
repeatable tests, but it does not by itself enforce code-quality
standards. A test that exercises production code does not catch
production code that:
- calls `.unwrap()` and panics under load,
- depends on an audit vulnerability in a transitive dep,
- formats inconsistently with the rest of the codebase,
- panics on input that the unit tests never exercised (the tests
  were written to match the code, not to verify behaviour).

This change establishes **quality engineering infrastructure** that
prevents these regressions:
- Lint deny for `unwrap` / `expect` / `panic` / `todo` /
  `unimplemented` in non-test code (clippy's `unwrap_used`,
  `expect_used`, `panic_used`, `todo` lints).
- Format enforcement via `cargo fmt --check` in CI.
- Dependency advisories via `cargo audit`.
- A coverage baseline via `cargo llvm-cov` (informational in v0.1).
- A documentation linter that rejects broken intra-doc links.

The principle (see `Agents.md`):

> Use open-source tools to implement checks that prevent runtime
> panics caused by issues like `unwrap`.

## What Changes

- New `clippy.toml` at the repo root with `disallowed-methods`
  rules and lint levels for `unwrap_used`, `expect_used`,
  `panic_used`, `todo`, `unimplemented`.
- New `Cargo.toml` workspace-level `[lints.rust]` section that sets
  `unsafe_code = "forbid"` for production crates (allows in tests).
- New `scripts/check-quality.sh` that runs clippy, fmt-check, audit,
  and doc-link checks; integrates with `scripts/check-tests.sh` from
  the TDD change.
- New `.cargo/config.toml` (informational) registering
  `RUSTFLAGS="-D warnings"` for the workspace.
- New `.github/workflows/ci.yml` (informational; no CI server yet)
  that runs the quality + test suites on every push.
- Updates to `Agents.md` documenting the new lints and the rationale.

## Capabilities

### New Capabilities

- `lint-policy` — clippy deny for unwrap/expect/panic in production
  code; allow in tests; CI-enforced.
- `format-policy` — `cargo fmt --check` on every commit.
- `dependency-audit` — `cargo audit` on every commit; advisories
  block the build.
- `doc-link-check` — intra-doc links resolve; broken `[]` paths fail.
- `unsafe-policy` — `unsafe_code = "forbid"` in production crates;
  allow in tests.

### Modified Capabilities

- `architecture` — adds the lint and policy enforcement; documented
  in the layered architecture diagram's "build & quality" stage.

## Impact

- **New file**: `clippy.toml` — workspace lint config.
- **New file**: `scripts/check-quality.sh` — single-entry shell script.
- **New file**: `.github/workflows/ci.yml` — informational GitHub
  Actions workflow.
- **New file**: `.cargo/config.toml` — workspace-wide cargo config.
- **Modified**: root `Cargo.toml` — `[workspace.lints.rust]` section.
- **Modified**: root `README.md` — add "Quality" section.
- **Modified**: `Agents.md` — new section on quality enforcement.
- **Modified**: every existing source file in `openpanel-app`,
  `openpanel-api`, `openpanel-cli`, `openpanel-agent` that uses
  `unwrap` / `expect` / `panic` — MUST be rewritten to use `?` or
  `match`. Test code is exempt.

## Notes on scope

The user's instruction was explicit:

> tests must be repeatable, and you must not make assumptions about
> the state of the database environment. Writing tests merely to
> match existing code is strictly prohibited. Remember, tests exist
> to verify correctness, not to accommodate the code so it passes
> checks; tests act as overseers of the code, not its allies. For any
> future code-related specs, testing tasks must be explicitly
> defined, listed first, and be detailed and actionable (this should
> be added as a principle in Agents.md). Additionally, use
> open-source tools to implement checks that prevent runtime panics
> caused by issues like `unwrap`. You need to add test cases to both
> future and existing specs, ensuring edge cases and error
> conditions are covered.

This spec operationalises the panic-prevention half of that
instruction. The TDD spec (`add-tdd-infrastructure`) operationalises
the test-quality half.