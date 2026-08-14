# Add file-length lint and auto-refactor pipeline

## Why

`openspec/specs/quality/spec.md` defines the existing per-script
quality gates (`check-fmt`, `check-clippy`, `check-docs`,
`check-audit`, `check-tests`). It does not pin a
**per-file line-count limit**. Without one, a single module is
free to balloon; review, navigation, and rebase-friendliness
all collapse — and Rust has no native answer for "this file is
too large for a human to navigate" the way it does for
`clippy::too_many_arguments`. cPanel and Baota both ship
human-readable file sizing; OpenPanel currently has nothing.

The threshold must be **configurable** (some bounded contexts,
e.g. shell-process integration tests, are intentionally long).
The default is **850 lines per file**. Any `.rs` file exceeding
the threshold must be split — **manually** by the developer or
**automatically** via `splitrs` (a Rust-AST-based file splitter
that consumes the same config file the lint reads). The check
must run inside the existing `make check` pipeline (mirroring the
audit gate's graceful-skip behaviour when the tool is missing
on a fresh machine), and the make script must
**auto-install** both `cargo-lint-extra` and `splitrs` during
its init phase so a developer who has never run the gate
before ends up with the toolchain intact after one command.

## What Changes

- New bounded context `file-length-pipeline` within the existing
  `quality` capability. Concretely:
- New `scripts/check-file-length.sh` that runs `cargo lint-extra
  check` and prints the standard `step: file-length status: ...`
  line consumed by `scripts/lib/step.sh`.
- New `lint-extra.toml` at the repo root declaring
  `soft_limit = 850`, `hard_limit = 1000`, and a configurable
  `exclude` glob list. Both numbers are overridable per
  environment via `OPENPANEL_FILE_LENGTH_SOFT_LIMIT` /
  `OPENPANEL_FILE_LENGTH_HARD_LIMIT`.
- New `.splitrs.toml` at the repo root: `max_lines = 850`,
  `max_impl_lines = 400`, `split_impl_blocks = true`,
  `preserve_comments = true`, `format_output = true`, with
  `_types` / `_impl` naming convention.
- New `[workspace.metadata.lint-extra]` block in `Cargo.toml`
  pointing at `lint-extra.toml`.
- `Makefile` additions:
  - `make install-lint-tools` — installs `cargo-lint-extra` and
    `splitrs` via `cargo install` (idempotent, logs versions).
  - `make split FILE=<path>` — invokes `splitrs --dry-run` by
    default; without `FILE` it lists oversized files (the same
    heuristic `lint-extra` uses).
  - `make check` — gains a new step that **first calls
    `make install-lint-tools` if either tool is missing**,
    matching the user's "auto-install during init" requirement;
    local machines and CI both end up with the tools after one
    `make check`. If install fails (offline / sandbox), the step
    degrades gracefully the way the audit gate does.
- New `.github/workflows/file-length.yml` that caches the
  tools, runs `make check`, and posts a GitHub status check
  visible on PRs.

## Capabilities

### Modified Capabilities

- `quality`: tiered file-length lint thresholds and an
  AST-aware optional auto-refactor, integrated into the
  per-script quality pipeline so every existing gate stays
  runnable in isolation.

## Impact

- New scripts: `scripts/check-file-length.sh`.
- New configs: `lint-extra.toml`, `.splitrs.toml`.
- New Cargo metadata: `[workspace.metadata.lint-extra]` line
  in the root `Cargo.toml`.
- New Makefile targets: `install-lint-tools`, `split`,
  integration points inside `check`.
- New CI: `.github/workflows/file-length.yml`.
- New domain exports: `FileLengthThresholds` value object under
  `crates/openpanel-core/src/quality.rs` so future Rust code
  (e.g. a future pre-commit hook) can read the same numbers
  the script reads — avoids drift.
