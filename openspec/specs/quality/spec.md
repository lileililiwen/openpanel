# quality Specification

## Purpose
TBD - created by archiving change add-quality-engineering-infrastructure. Update Purpose after archive.
## Requirements
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

The repo MUST contain an `.cargo/audit.toml` (auto-managed) for
advisories the team has triaged. Each entry MUST have a `reason`.
Entries with a known upstream fix MUST have an `expires_on` date.

#### Scenario: Transitive dep has a known RUSTSEC advisory

- **WHEN** `cargo audit` detects `RUSTSEC-2024-XXXX` in the dep graph
- **THEN** the audit gate fails (`make check` exits non-zero) and the
  PR cannot be merged until the advisory is resolved or explicitly
  suppressed with an expiry.

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

A root `Makefile` MUST act as the quality gate *manager*: it knows the
gates and their order but holds no business logic. Each gate MUST live
in its own small script under `scripts/` and be runnable in isolation
via its own target (`make fmt`, `make clippy`, `make docs`, `make
audit`, `make test`). `make check` MUST run, in order:
1. `scripts/check-fmt.sh` — `cargo fmt --all -- --check`
2. `scripts/check-clippy.sh` — `cargo clippy --workspace --all-targets
   -- -D warnings`
3. `scripts/check-docs.sh` — `cargo doc --workspace --no-deps` (catches
   broken doc links)
4. `scripts/check-audit.sh` — `cargo audit` (skips gracefully when the
   tool is not installed)
5. `scripts/check-tests.sh` — `cargo test --workspace` (the TDD script)

The exit code of `make check` MUST be non-zero if any gate fails.
Output MUST be machine-parseable: each step on its own line as
`step: name status: ok | failed`, produced by a shared helper
(`scripts/lib/step.sh`).

#### Scenario: Single entry point

- **WHEN** a developer runs `make check` locally
- **THEN** all five gates execute in order; a failure in gate 1
  short-circuits the rest and `make` exits with a non-zero code.

#### Scenario: Isolated gate

- **WHEN** a developer runs `make clippy` alone
- **THEN** only the clippy gate runs; a passing run exits 0 without
  touching the other gates.

### Requirement: CI Workflow

A new `.github/workflows/ci.yml` MUST run on every push and PR:
1. Checkout, install Rust toolchain
2. Cache cargo registry and target/
3. Run `make check`
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

### Requirement: Configurable File-Length Thresholds

The `quality` capability SHALL enforce per-file line-count
thresholds on every `.rs` source file under the workspace. The
threshold pair is configurable: a `soft_limit` (default `850`)
emits a warning without failing, and a `hard_limit` (default
`1000`) fails `make check` with a non-zero exit. The effective
values are resolved in this strict precedence order:

1. Environment variables `OPENPANEL_FILE_LENGTH_SOFT_LIMIT` and
   `OPENPANEL_FILE_LENGTH_HARD_LIMIT` (positive integers).
2. The values declared in `lint-extra.toml` at the repo root.
3. The built-in defaults `850` and `1000`.

The resolution helper MUST be implemented once and consumed by
both the shell gate (`scripts/check-file-length.sh`) and the
Rust crate `openpanel-core::quality::FileLengthThresholds`,
proving that the values used by `make check` and by any future
Rust-side caller cannot drift.

#### Scenario: Default thresholds apply

- **WHEN** neither env vars nor `lint-extra.toml` set explicit values
- **THEN** `FileLengthThresholds::effective()` returns `soft_limit=850, hard_limit=1000`.

#### Scenario: Env vars override file

- **WHEN** `OPENPANEL_FILE_LENGTH_SOFT_LIMIT=600` is exported and `lint-extra.toml` declares `soft_limit=850`
- **THEN** the effective soft limit is `600` for the current shell; the file remains untouched.

#### Scenario: Env vars invalid

- **WHEN** `OPENPANEL_FILE_LENGTH_SOFT_LIMIT=0` is exported
- **THEN** the resolver ignores it and falls through to the file value; no panic, no warning beyond the existing audit-style skip.

### Requirement: Tiered Lint and Refactor Toolchain

The system SHALL use `cargo-lint-extra` (a thin wrapper around
`cargo check --all-targets` plus file-length diagnostics) for
the lint side and `splitrs` (Rust-AST-based file splitter) for
the optional auto-refactor side. The two tools MUST be installed
together via `make install-lint-tools`, which runs `cargo
install --locked cargo-lint-extra` and `cargo install --locked
splitrs`. The Makefile MAY call `install-lint-tools`
automatically when running `make check` if either tool is
missing on `PATH`.

#### Scenario: First run auto-installs

- **WHEN** a developer runs `make check` on a machine where neither tool is on `PATH`
- **THEN** `make check` invokes `make install-lint-tools` before the file-length step and emits a `bootstrap` line to stdout; if install succeeds the check proceeds; if install fails (offline / sandbox) the file-length step degrades gracefully the same way the audit gate does.

#### Scenario: Subsequent run skips install

- **WHEN** both tools are already on `PATH`
- **THEN** `make check` skips `install-lint-tools` and proceeds directly to the file-length step.

#### Scenario: Install is idempotent

- **WHEN** `make install-lint-tools` is run twice in a row
- **THEN** the second invocation is a no-op for the cargo registry (cargo's own idempotency) and the Makefile prints the cached versions on both invocations.

### Requirement: File-Length Check Shell Gate

A new `scripts/check-file-length.sh` SHALL run `cargo lint-extra
--config lint-extra.toml check` and emit a single
`step: file-length status: ok | failed | skipped` line via the
existing `scripts/lib/step.sh` helper. The script MUST exit `0`
when the tool is absent (graceful skip), `0` when every `.rs`
file in the workspace is at or below `soft_limit`, and `1`
when any `.rs` file is strictly above `hard_limit`. The script
MUST NOT emit plaintext secrets or change dependency state
beyond what `cargo lint-extra` itself changes.

#### Scenario: All files under soft limit

- **WHEN** no `.rs` file exceeds 850 lines
- **THEN** the script exits `0` and prints `step: file-length status: ok`.

#### Scenario: One file over hard limit

- **WHEN** `crates/openpanel-domain/src/foo.rs` is `1100` lines
- **THEN** the script exits `1`, prints `step: file-length status: failed`, and the trailing diagnostic names the file and its line count without revealing file contents.

#### Scenario: Tool missing

- **WHEN** `cargo-lint-extra` is not on `PATH`
- **THEN** the script exits `0` and prints `step: file-length status: skipped` followed by an installation hint (`make install-lint-tools`).

### Requirement: Exclude Glob Configuration

`lint-extra.toml` SHALL declare an `exclude` list with at
minimum: `**/target/**`, `**/vendor/**`, `**/scripts/lib/**`,
and `**/tests/common/**`. The list is the only authoritative
source for excluded paths; the shell gate MUST NOT silently
suppress failures from files matching a non-listed path. Each
entry in `exclude` MUST be accompanied by an inline comment
naming the reason for exclusion (e.g. `# generated code`,
`# vendored dependency`, `# shared test fixture`).

#### Scenario: Target directory excluded

- **WHEN** `crates/openpanel-domain/target/debug/foo.rs` contains 5000 lines
- **THEN** the lint does NOT report it as oversized; no warning, no error.

#### Scenario: Crate source not excluded

- **WHEN** `crates/openpanel-domain/src/foo.rs` contains 1100 lines
- **THEN** the lint reports it; the file is in the workspace's `crates/` tree and is not in `exclude`.

#### Scenario: Adding an entry is auditable

- **WHEN** a developer adds a new entry to `exclude`
- **THEN** `git diff lint-extra.toml` MUST surface a comment explaining the reason for the exclusion; code review rejects adds without a justification comment.

### Requirement: splitrs Configuration and Dry-Run Default

`.splitrs.toml` at the repo root SHALL declare `max_lines =
850`, `max_impl_lines = 400`, `split_impl_blocks = true`,
`preserve_comments = true`, `format_output = true`, and
`_types` / `_impl` naming suffixes. A new `make split` target
SHALL default to `--dry-run` for safety; running with `FILE=
<path>` invokes `splitrs --input <path> --output <dir>/<base>/
--config .splitrs.toml --dry-run` and prints the planned
splits without modifying the file system. Running without
`FILE` lists oversized files instead.

#### Scenario: Dry-run prints plan

- **WHEN** an Owner runs `make split FILE=crates/openpanel-domain/src/foo.rs`
- **THEN** `splitrs` is invoked with `--dry-run`; the planned
        modules and their line counts are printed; no files are
        written.

#### Scenario: Listing oversized files

- **WHEN** `make split` is run without `FILE`
- **THEN** the target prints a sorted list of files over
        `max_lines` (the configured soft limit), each with its
        line count and path.

#### Scenario: splitrs not installed

- **WHEN** `make split` is run and `splitrs` is not on `PATH`
- **THEN** the target prints `splitrs not installed. Run 'make install-lint-tools' first.` and exits non-zero without invoking `splitrs`.

### Requirement: make check Integration

The root `Makefile`'s `check` target SHALL depend on a new
`check-file-length` target that runs `scripts/check-file-length.sh`.
The dependency chain `fmt → clippy → docs → audit → test →
ensure-lint-tools → check-file-length` SHALL preserve
existing behaviour: any earlier step's failure short-circuits
the rest with the same exit code as before. The order MUST be
documented in the Makefile's leading comment so a reader can
re-derive the chain without reading the file.

#### Scenario: New gate runs after others

- **WHEN** a developer runs `make check`
- **THEN** the existing gates execute first; only on their success does `ensure-lint-tools` run, followed by `check-file-length`; the final banner `=== All quality checks passed ===` is printed only when the new step is `ok` or `skipped`.

#### Scenario: Existing step failure unchanged

- **WHEN** `make check` is run and `cargo fmt` finds diffs
- **THEN** `make` exits before `ensure-lint-tools` and
        `check-file-length` are reached; the existing exit code is preserved.

#### Scenario: Isolated invocation

- **WHEN** a developer runs `make check-file-length` alone
- **THEN** only the file-length gate runs; the other gates do NOT execute; the output matches the inside of `make check`.

### Requirement: File-Length CI Workflow

A new `.github/workflows/file-length.yml` SHALL run on
`pull_request`. The workflow installs Rust stable, runs `make
install-lint-tools` once, then runs `make check-file-length`.
A failure MUST be reported as a GitHub status check visible
on PRs. The workflow MUST NOT modify any source file.

#### Scenario: Pull request fails hard limit

- **WHEN** a PR introduces a `.rs` file at 1100 lines
- **THEN** the `file-length` workflow fails; the PR check is
        red and the merge button is blocked.

#### Scenario: Tools absent on runner

- **WHEN** the workflow starts on a fresh runner
- **THEN** `make install-lint-tools` is invoked first; if it
        fails the workflow fails before any check runs;
        otherwise the lint runs.

#### Scenario: Workflow is read-only on PRs

- **WHEN** the workflow runs on a PR
- **THEN** it MUST NOT push commits or modify files; the
        workflow runs in `pull_request` scope only and the job
        runs with `permissions: contents: read`.

### Requirement: Goal — Maintainability and Readability

The pipeline SHALL exist to keep files small enough that a
developer can find any section in under a minute of scrolling.
The `soft_limit` is a warning that encourages proactive
splits; the `hard_limit` is a hard ceiling that prevents
unreadable files from landing in the tree. A file that
exceeds `hard_limit` MUST be split — either manually by the
developer, or with `make split FILE=<path>` followed by
reviewing the dry-run output and committing the resulting
modules. No automated process SHALL silently rewrite source
files; every split is a human-reviewed change.

#### Scenario: Manual split is the canonical path

- **WHEN** a file exceeds `hard_limit`
- **THEN** the developer MAY split by hand (preferred), or
        MAY use `make split FILE=<path>` to preview; in both
        cases the resulting modules MUST still pass `make
        check` end-to-end.

#### Scenario: Dry-run is the only automation

- **WHEN** `make split FILE=<path>` is invoked
- **THEN** the tool runs in `--dry-run` mode by default; no
        files are modified; to actually execute the split the
        developer MUST run `splitrs` without `--dry-run`
        explicitly, after reviewing the dry-run output.

#### Scenario: Drift prevented

- **WHEN** a developer changes `lint-extra.toml` or
        `.splitrs.toml`
- **THEN** the file-length gate MUST keep passing (no
        workspace source file is silently exempt unless
        explicitly listed with a justification); no other
        gate is affected.

### Requirement: Template-Literal Scan

The repository SHALL ship a content scanner that fails `make check` whenever a literal hex / rgb / hsl colour value or a literal user-visible English string appears in any path that has not been approved by the web-ui spec. The scanner's allowlist SHALL be `tokens.css` for colours and `t.rs` for strings; every other path that contains a literal is a violation. The scanner MUST be included in `make check`.

#### Scenario: Literal hex in a template

- **WHEN** a developer writes `style="color: #4f8cff"` inside a `maud` template
- **THEN** `make check` fails with `scan: literal hex color outside tokens.css` and the offending path and line number.

#### Scenario: Same literal in tokens.css

- **WHEN** the same colour is declared inside `tokens.css`
- **THEN** the scanner accepts it.

#### Scenario: Literal user-visible string in a template

- **WHEN** a developer writes `<button>Save</button>`
- **THEN** the scanner fails with `scan: literal user-visible string in template body`.

### Requirement: i18n Disallowed Formatters

`clippy.toml` SHALL declare a `disallowed-methods` entry for any locale-ignorant formatter introduced by the follow-on i18n change; in this change, only English-only `format!` in code paths that take a user-facing context is restricted. No production call site may construct the user-visible sentence by raw `format!()` once the `add-i18n-and-localization` change ships; users of `format!` for logs, errors, or internal formatting continue to be allowed.

#### Scenario: Log message uses format

- **WHEN** an internal log line uses `format!("disk used {} bytes", used)`
- **THEN** linting is silent (allowed).

#### Scenario: User-visible string uses format

- **WHEN** a developer introduces `format!("Welcome, {}!", user)` for a UI sentence
- **THEN** the `add-i18n-and-localization` change's lint rejects it and the call site rewrites to `f!("welcome_user", user)`.

### Requirement: Accessibility Gate

The repository SHALL ship a `make a11y` target that runs axe-core against the running dev server when available. The gate MUST be included in CI but MUST skip when no dev server is reachable, preserving local-friendliness.

#### Scenario: Dev server reachable

- **WHEN** `make a11y` runs and `http://127.0.0.1:8080/healthz` returns 200
- **THEN** axe-core runs against the reachable pages and exits 0 only if no serious or critical violations are present.

#### Scenario: Dev server unreachable

- **WHEN** `make a11y` runs and `127.0.0.1:8080` is closed
- **THEN** the target prints `a11y: skipped (no dev server)` and exits 0.

### Requirement: Reuse / Duplication Gate

`make check` SHALL run `scripts/check-reuse.sh`, which detects a public
item defined in more than one crate (excluding `#[cfg(test)]` modules).

Function-name equality across crates is a weak signal on its own
(`router()`, `register()`, `iter()` recur per module), so the gate
**reports** any cross-crate duplicate as a **warning** by default and
does NOT fail the build. Under `scripts/check-reuse.sh --strict` (CI for
new changes, and local pre-commit) a cross-crate duplicate `pub fn`
(outside a trivial-name denylist) FAILS, forcing the author to reuse
the existing implementation instead of introducing a second one.

This directly addresses the "failure to reuse existing logic" failure
mode as a mechanical backstop; the primary defense is the Explore &
Reuse workflow step in `Agents.md`. The script MUST degrade to
`status: skipped` when `rg` is unavailable.

#### Scenario: Same function defined twice (strict)

- **WHEN** `pub fn hash_password` exists in both `openpanel-core` and
  `openpanel-app` and the gate runs in `--strict` mode
- **THEN** the step fails naming both crates, and the author must
  delete one and reuse the other.

#### Scenario: Trivial function name is not flagged

- **WHEN** `pub fn router` exists in many crates
- **THEN** the gate does not fail it (it is in the denylist).

#### Scenario: Default mode reports, does not fail

- **WHEN** the gate runs in default mode on a tree with cross-crate
  duplicates
- **THEN** it prints warnings and exits 0.

#### Scenario: Tool absent

- **WHEN** `rg` is not installed
- **THEN** the step prints `step: reuse status: skipped` and exits 0.

### Requirement: Intra-Crate Layering Gate

The four-layer DDD architecture is enforced structurally for the core
crates, but new bounded contexts are folders *inside*
`openpanel-domain` / `openpanel-app`, which cargo cannot isolate.
`make check` SHALL run `scripts/check-layering.sh`, which asserts:
- no file under `crates/openpanel-domain/src/**` contains `use
  openpanel_app` or `use openpanel_api`,
- no file under `crates/openpanel-app/src/**` contains `use
  openpanel_api`.

A violation prints `step: layering status: failed` with the offending
file and exits non-zero. The script MUST degrade to `status: skipped`
when `rg` is unavailable.

#### Scenario: Domain imports app code

- **WHEN** a domain module adds `use openpanel_app::some_service`
- **THEN** `make check` fails at the `layering` step naming the file;
  the import must be removed (move the logic or use a trait).

#### Scenario: App imports api code

- **WHEN** an app service imports `openpanel_api`
- **THEN** the `layering` step fails.

### Requirement: Spec-To-Test Drift Gate

`make check` SHALL run `scripts/check-spec-test-drift.sh`, which walks
every archived spec under `openspec/specs/*/spec.md`, counts its
`#### Scenario:` entries, and verifies that at least one test in the
workspace references the capability (by grepping for the capability
name in `tests/` and `#[cfg(test)]` modules). It prints a per-spec
report and, under `--strict` (used for specs created after this
change), fails when a scenario has no covering test. Pre-existing specs
are reported but do not fail the build, to avoid retroactive breakage.

#### Scenario: New spec has no test

- **WHEN** a spec archived after this change declares scenarios but no
  test references its capability
- **THEN** `make check` fails at the `spec-test-drift` step.

#### Scenario: Pre-existing spec gap is reported, not failed

- **WHEN** an older spec has scenarios with no covering test
- **THEN** the script prints a warning line but exits 0.

### Requirement: Archive-Time Delta Merge Gate

The quality pipeline SHALL verify that every requirement introduced by
an archived OpenSpec change's deltas exists in the corresponding live
spec under `openspec/specs/<cap>/spec.md`. Pre-existing gaps SHALL be
recorded in a committed baseline file and reported without failing;
any drift not present in the baseline SHALL fail `make check` naming
the capability, requirement, and originating archive path. The
baseline SHALL only shrink over time.

#### Scenario: New un-merged delta fails the build

- **WHEN** an archived change's delta contains a requirement absent
        from both the live spec and the baseline
- **THEN** `check-spec-drift` exits non-zero and names both files.

#### Scenario: Baseline gap reported without failing

- **WHEN** a missing requirement is listed in the committed baseline
- **THEN** the gate reports it as tracked debt and exits 0.

#### Scenario: Clean tree passes

- **WHEN** every archived delta's requirements are present in their
        live specs
- **THEN** the gate exits 0 without modifying any file.

