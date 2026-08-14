## ADDED Requirements

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
