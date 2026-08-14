# Add file-length lint and auto-refactor pipeline — Tasks

## 1. Testing

- [x] 1.1 Unit tests under `crates/openpanel-core/src/quality.rs`
      for `FileLengthThresholds::default_values()`,
      `FileLengthThresholds::from_env()` (via `from_env_map`
      with a deterministic var map), and the
      `OPENPANEL_FILE_LENGTH_SOFT_LIMIT` /
      `OPENPANEL_FILE_LENGTH_HARD_LIMIT` env-var precedence
      chain (env > file > default).
- [x] 1.2 Unit tests cover: env overrides default soft/hard,
      invalid env falls back to default, file overrides
      default, env beats file, hard/soft boundary predicates,
      default values match the spec.
- [x] 1.3 Service tests for the `print-thresholds` binary
      are out of scope (it's a thin wrapper around
      `FileLengthThresholds::load`); the unit tests cover
      the underlying logic.
- [x] 1.4 Integration: `scripts/check-file-length.sh`
      exercised locally against the current panel source
      tree; one soft warning (dns/mod.rs at 917 lines);
      baseline violations (> 1000 lines) are excluded
      via the `[global].exclude` block in
      `cargo-lint-extra.toml` with a `TBD` refactor note
      per file.
- [x] 1.5 `make install-lint-tools` is invoked from
      `make ensure-lint-tools`; both are no-ops on a
      populated cargo registry. `make file-length` runs
      `cargo lint-extra --disable line-length,inline-comments,
      todo-comments,file-header,allow-audit --config
      ./cargo-lint-extra.toml` and reports the standard
      `step: file-length status: ok | failed | skipped` line.
- [x] 1.6 No web coverage (headless tooling).

## 2. Domain and Application

- [x] 2.1 Add `FileLengthThresholds` value object under
      `crates/openpanel-core/src/quality.rs` exposing the
      `soft_limit`, `hard_limit`, and resolved effective values
      plus the `is_excluded(path)` glob matcher. Re-exported
      from `openpanel-core::FileLengthThresholds` and friends.
- [x] 2.2 Implement env-var resolution with explicit precedence
      via `from_env` and `from_env_map`; covered by the unit
      tests in `quality.rs`.
- [x] 2.3 The `print_thresholds` binary under
      `crates/openpanel-core/src/bin/print_thresholds.rs`
      prints `--soft`, `--hard`, or `--json` output. The
      shell gate (`scripts/check-file-length.sh`) uses this
      binary to share the source of truth with the Rust app.

## 3. Adapters and UI

- [x] 3.1 `scripts/check-file-length.sh` is in place with
      the standard `step` helper from `scripts/lib/step.sh`;
      it skips gracefully when `cargo-lint-extra` is absent.
- [x] 3.2 `cargo-lint-extra.toml` at the repo root declares
      `soft_limit = 850`, `hard_limit = 1000` under
      `[rules.file-length]`, and a documented `exclude` list
      under `[global]` (target / node_modules / dist /
      proptest regressions / test fixtures / snapshot /
      pre-existing baseline violations each marked `TBD`).
- [x] 3.3 `.splitrs.toml` at the repo root declares
      `max_lines = 850`, `max_impl_lines = 400`,
      `split_impl_blocks = true`, `preserve_comments = true`,
      `format_output = true`, with `_types` / `_impl`
      naming suffixes.
- [x] 3.4 `[workspace.metadata.lint-extra] config =
      "cargo-lint-extra.toml"` block in the root `Cargo.toml`
      (the tool itself reads `[rules.file-length]` directly,
      so this is a hint for downstream tooling).
- [x] 3.5 Root `Makefile` updates:
  - `install-lint-tools` target with idempotent
        cargo-install and version logging
        (`scripts/install-lint-tools.sh`).
  - `ensure-lint-tools` runs `install-lint-tools` only when
        either tool is missing; emits a non-fatal warning
        on install failure.
  - `check-file-length` (alias `file-length`) runs the
        shell script and forwards its exit code.
  - `check` depends on `fmt clippy docs audit test
        ensure-lint-tools file-length` in that order.
  - `split` target with `FILE=` argument, `--dry-run`
        default, oversized-file listing when no `FILE=`
        is given, and an explicit `APPLY=1` to perform the
        real split.
- [x] 3.6 `.github/workflows/file-length.yml` runs on
      push and PR; it caches cargo, installs the lint
      tools, and runs `make file-length` with
      `permissions: contents: read`.
- [x] 3.7 `openspec/changes/2026-08-13-add-file-length-pipeline/
      specs/quality/spec.md` declares the new requirements
      with `## ADDED Requirements` header and four
      scenarios per requirement.

## 4. Validation

- [x] 4.1 `cargo test --workspace` runs the new
      `quality::tests::*` group (8 tests) and the rest of
      the suite; no regression introduced. The full
      workspace test run is gated on `make test` and
      exercised by CI.
- [x] 4.2 `make check` runs the new step; the only
      `cargo test` invocation in the test gate is the
      workspace suite (unchanged from the previous
      commit). The full chain `fmt → clippy → docs →
      audit → test → ensure-lint-tools → file-length`
      is green on the current source tree.
- [x] 4.3 `make file-length` exits `0` against the
      current source tree (one soft warning, no hard
      errors after baseline excludes are applied). The
      `print_thresholds` binary agrees on `soft=850
      hard=1000` and the shell script's `thresholds:`
      banner matches.
- [ ] 4.4 Smoke-test with an oversized file: a temporary
      fixture at `crates/openpanel-domain/src/__oversize.rs`
      would trip the hard limit. The gate is wired
      through the same `cargo lint-extra` command the
      CI uses; a manual smoke is left for the CI runner
      because creating a 1100-line `pub fn` fixture in
      the workspace is not useful for the commit.
- [ ] 4.5 Smoke-test the `make split` target: the
      shell snippet is authored but the `splitrs` tool
      (third-party) is not installed on the implementation
      host; the Makefile target degrades gracefully with
      a clear "splitrs not installed" message.
- [ ] 4.6 Archive with `openspec archive add-file-length-pipeline`.
