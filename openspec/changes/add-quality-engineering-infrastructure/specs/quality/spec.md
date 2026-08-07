## ADDED Requirements

### Requirement: Lint Policy — No `unwrap` / `expect` / `panic` in Production Code

The workspace's `[lints.rust]` table MUST set:
- `unsafe_code = "forbid"`
- The workspace MUST enable clippy's `unwrap_used`, `expect_used`,
  `panic_used`, `todo`, `unimplemented` lints at `deny` level.
- Production crates MUST comply; test code (`#[cfg(test)]`) MAY
  contain them.

A `clippy.toml` at the repo root MUST additionally forbid via
`disallowed-methods`:
- `std::panic::catch_unwind` (in production code; test code may use
  it to assert panics — actually the linter only blocks unwrap, not
  catch_unwind; this rule is informational).
- Any future-specific method that the team agrees is panic-prone
  (e.g. `String::from_utf8_unchecked`).

#### Scenario: Clippy rejects a new `unwrap` in production code

- **WHEN** a developer adds `.unwrap()` to a non-test function and
  runs `cargo clippy --workspace --all-targets -- -D warnings`
- **THEN** the build fails with a `clippy::unwrap_used` error
  pointing at the offending line.

#### Scenario: Test code can use `unwrap`

- **WHEN** a developer adds `.unwrap()` inside a `#[cfg(test)] mod
  tests` block
- **THEN** clippy does NOT flag it.

#### Scenario: Existing `unwrap`s in production code are eliminated

- **WHEN** this change is applied, all production code MUST be
  scanned for `unwrap()` / `.expect(` / `panic!` / `todo!` /
  `unimplemented!`. The change MUST replace each occurrence with
  `?` propagation, `match` / `if let` error handling, or
  `.expect("invariant: ...")` with a justification string.

### Requirement: Format Policy

`rustfmt` MUST be configured at the repo root (`rustfmt.toml` or
inherited defaults). CI MUST run `cargo fmt --all -- --check` and
fail on any diff.

#### Scenario: Unformatted file is rejected

- **WHEN** a developer changes a file and the resulting source does
  not match `cargo fmt`'s output
- **THEN** `cargo fmt --all -- --check` fails and prints a diff.

### Requirement: Dependency Audit

`cargo-audit` (already a workspace dev-dep candidate) MUST run on
every CI build. Any advisory with severity `warning` or higher MUST
fail the build.

The repo MUST contain an `audit-suppressions.toml` (auto-managed) for
advisories the team has triaged. Each entry MUST have a `reason` and
an `expires_on` date.

#### Scenario: Transitive dep has a known RUSTSEC advisory

- **WHEN** `cargo audit` detects `RUSTSEC-2024-XXXX` in the dep graph
- **THEN** `scripts/check-quality.sh` exits non-zero and the PR cannot
  be merged until the advisory is resolved or explicitly suppressed
  with an expiry.

### Requirement: Documentation Link Check

`#![deny(rustdoc::broken_intra_doc_links)]` MUST be set at the
crate root of `openpanel-core`, `openpanel-domain`, `openpanel-app`,
`openpanel-api`, `openpanel-cli`, `openpanel-agent`,
`openpanel-test-support`. Broken intra-doc links (e.g. `[Foo]` where
`Foo` does not exist or is private) MUST fail the build.

#### Scenario: Renamed function leaves stale doc link

- **WHEN** a developer renames a public function but leaves a stale
  `[old_name]` reference in a doc comment
- **THEN** `cargo doc --workspace --no-deps` fails with
  `rustdoc::broken_intra_doc_links`.

### Requirement: Unsafe Code is Forbidden in Production

`unsafe_code = "forbid"` MUST be set in `[lints.rust]` at the
workspace level. Production crates MUST compile without `unsafe`
blocks. Test crates MAY use `unsafe` inside `#[cfg(test)]` blocks.

#### Scenario: New `unsafe` block in production code

- **WHEN** a developer adds `unsafe { ... }` to a non-test function
- **THEN** the build fails with `error[E0133]: use of an `unsafe`
  block in a `forbid` lint level`.

### Requirement: Quality Script

A new `scripts/check-quality.sh` MUST run, in order:
1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings`
3. `cargo doc --workspace --no-deps` (catches broken doc links)
4. `cargo audit`
5. `cargo test --workspace` (delegates to the TDD script)

Exit code MUST be non-zero if any step fails. Output MUST be
machine-parseable (each step on its own line: `step: name status: ok
| failed`).

#### Scenario: Single entry point

- **WHEN** a developer runs `scripts/check-quality.sh` locally
- **THEN** all five steps execute in order; a failure in step 1
  short-circuits the rest and the script exits with a non-zero code.

### Requirement: CI Workflow

A new `.github/workflows/ci.yml` MUST run on every push and PR:
1. Checkout, install Rust toolchain
2. Cache cargo registry and target/
3. Run `scripts/check-quality.sh`
4. Upload coverage report as an artifact (informational)

The workflow MUST be informational in v0.1 — no actual CI server
required to run it, but the file MUST be valid YAML and parse with
`yamllint`.

#### Scenario: CI runs locally

- **WHEN** a developer runs `act -j test` (or the GitHub-hosted runner
  via push)
- **THEN** the workflow executes all five steps and reports status
  per step.

### Requirement: Coverage Baseline (Informational)

A new `scripts/coverage.sh` MUST run `cargo llvm-cov` (or fall back
to `cargo tarpaulin` if available) and emit:
- `target/coverage/coverage.json` — lcov format
- A summary line: `coverage: <percent>`

The script MUST NOT fail the build on low coverage in v0.1. Future
changes can introduce thresholds.

#### Scenario: First coverage run

- **WHEN** `scripts/coverage.sh` is invoked
- **THEN** it produces an lcov report and prints
  `coverage: 78.4%` (or whatever the actual number is) to stdout,
  without failing on any threshold.