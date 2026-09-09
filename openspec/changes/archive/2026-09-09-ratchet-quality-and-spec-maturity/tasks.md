# Tasks: Ratchet quality and spec maturity

## 1. Testing

- [x] Add clean and violating fixtures for strict spec-test drift.
  Four new `assert` blocks in `scripts/test-gates.sh`
  (`spec-test-drift: baseline-tracked gap + new covered cap passes
  (strict)`, `... new uncovered cap not in baseline fails (strict)`,
  plus the four reuse / coverage / maturity pairs below). All
  fixtures were added BEFORE the gate scripts were modified, so the
  self-test was red on first run; the spec-test-drift pair is now
  green after the script learned to read the
  `OPENSPEC_SPEC_TEST_DRIFT_BASELINE` env var.
- [x] Add clean and violating fixtures for strict reuse and baseline ratchets.
  `scripts/test-gates.sh` gained a `reuse-classified` block: the
  positive fixture writes two cross-crate duplicates (`compute_widget`,
  `brand_new_dup`) and lists both in the classified baseline; the
  negative fixture drops `brand_new_dup` from the baseline and
  asserts `--strict` fails.
- [x] Add coverage-threshold fixtures for tool-present, tool-missing, and below-floor cases.
  `scripts/test-gates.sh` gained a `coverage-floor` block with four
  assertions: tool-missing + not-required (passes), measured-coverage
  below floor (fails), measured-coverage above floor (passes), and
  tool-missing + required (fails). A synthetic `LF:10 LH:3` lcov
  report is enough to drive the line-coverage percentage to 30%.
- [x] Add tests proving accepted TODO/fake/blocker records are explicit and unlisted records fail.
  `scripts/test-gates.sh` gained a `maturity_evidence` block: an
  empty manifest passes, a `// TODO(openpanel#TEST)` source line
  with a matching record passes, the same source with an empty
  manifest fails.
- [x] Run the fixture suite red before implementing gates.
  Confirmed: `TEST_GATES_REENTRY=1 ./scripts/test-gates.sh` produced
  12 red assertions (the four new fixture groups + three pre-existing
  `make check` wiring checks that turned out to be a latent
  `pipefail` issue, fixed in the same change). All 65 are now green.

## 2. Implementation

- [x] Extend `check-spec-test-drift.sh` with strict new/modified capability handling and baseline updates.
  Added `--strict` and `--write-baseline` modes. The baseline
  (`openspec/specs/.spec-test-drift-baseline`) is read when present;
  capabilities listed in it are reported as tracked debt under
  strict mode and pass. The pre-existing default mode and the
  legacy strict mode (no baseline ⇒ fail on any gap) are preserved
  so every prior test fixture still works.
- [x] Add configured coverage floors and a mandatory CI coverage command.
  New `scripts/check-coverage-floor.sh` parses the lcov report,
  computes line-coverage percentage, and compares to
  `OPENPANEL_COVERAGE_FLOOR` (default 60). The
  `OPENPANEL_COVERAGE_REQUIRED=1` mode (set by the new
  `coverage-floor` CI job) makes tool-missing fail instead of skip.
  Wired into `make check` between `governance-contract` and
  `maturity`.
- [x] Add strict reuse classification and wire it into `make check`.
  `scripts/check-reuse.sh` now reads
  `openspec/governance/.reuse-classified-baseline` and exempts
  classified names in `--strict` mode. The mandatory `make check`
  target runs `reuse-strict` (new) instead of `reuse`. Initial
  baseline generated from the current tree (one entry per existing
  duplicate, with a per-type reason for the 9 cross-crate type
  duplicates and a per-aggregate reason for the ~136 function
  duplicates). The `--write-baseline` mode is also available for
  future reclassification.
- [x] Add an evidence manifest and checker diagnostics for stubs, fakes, and environment blockers.
  New `openspec/governance/evidence.yaml` (YAML, `version: 1`).
  New `scripts/check-maturity.sh` scans `crates/` for
  `// TODO(openpanel#<id>[: ...])`, `// FIXME(...)`, and
  `// stub(...)` markers, then matches each id against the manifest.
  An untracked marker fails the build and prints at least one source
  location; a stale record is advisory. The ACME HTTP-01 marker at
  `crates/openpanel-app/src/ssl/acme.rs:102` is the first
  registered record, with `closure:
  complete-production-acme-lifecycle`.
- [x] Update `quality`, `testing`, and `agent-quality` specifications without weakening protected requirements.
  Three delta specs added under
  `openspec/changes/ratchet-quality-and-spec-maturity/specs/`:
  `quality/spec.md` (3 new requirements: strict spec-test drift
  ratchet, strict reuse classification, coverage floor + tool
  availability), `testing/spec.md` (1 new requirement: production
  incomplete work has evidence), and `agent-quality/spec.md` (1 new
  requirement: new maturity gates have executable protection). All
  deltas are `## ADDED Requirements` blocks; the existing
  governance-protected requirements are not modified, so their
  manifest digests are unchanged.

## 3. Verification

- [x] Run focused gate self-tests and `make test-gates`.
  `make test-gates` reports `gate self-tests: 65 passed, 0 failed`.
  The pre-existing `make check` wiring assertions (3 latent
  `pipefail` failures in `scripts/test-gates.sh`) were fixed in the
  same change by capturing `make -n check` into a variable once and
  grepping the variable, so the pipeline no longer returns the
  broken-pipe exit code of `make`.
- [x] Run `openspec validate ratchet-quality-and-spec-maturity --strict`.
  `Change 'ratchet-quality-and-spec-maturity' is valid`. The new
  capability `quality-maturity-ratchet` is registered (5 new
  requirements, 5 scenarios) and the three delta specs
  (`quality`, `testing`, `agent-quality`) all add new
  requirements cleanly.
- [x] Run `make check` and record any external-tool or runtime blocker separately.
  `make check` exits zero with the canonical
  `=== All quality checks passed ===` banner. The pre-existing
  rustdoc warnings (unclosed HTML tags in
  `crates/openpanel-core/src/bin/print_thresholds.rs:11` and
  `crates/openpanel-domain/src/web_application_installer/mod.rs:268`)
  are unchanged by this work; the docs gate is intentionally
  warning-only for these (`scripts/check-docs.sh` does not pass
  `-D warnings`), so they are not blockers.
- [x] Update `docs/TODOS.md` and governance baselines only with evidenced entries.
  The `docs/TODOS.md` ACME entry (entry #1) is unchanged; it
  remains the source of truth for the ACME HTTP-01 follow-up. The
  new `openspec/governance/evidence.yaml` is a machine-readable
  companion to `docs/TODOS.md`: the ACME entry's id
  (`ACME-HTTP01`) is the first registered record. The
  `openspec/governance/.reuse-classified-baseline` and
  `openspec/specs/.spec-test-drift-baseline` were generated from
  the current tree (`scripts/check-reuse.sh --write-baseline` and
  `scripts/check-spec-test-drift.sh --write-baseline`) and reviewed
  in this change.
