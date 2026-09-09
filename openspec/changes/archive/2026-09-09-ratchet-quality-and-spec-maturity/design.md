# Design: Ratchet quality and spec maturity

## Approach

Extend the existing shell gates instead of introducing a second quality
framework. `check-spec-test-drift.sh` will distinguish new/modified specs from
the recorded baseline, `coverage.sh` will enforce a configured floor in CI,
and `check-reuse.sh --strict` will become part of the mandatory chain after
existing duplicates are explicitly classified. A small evidence manifest will
classify accepted test fakes, tracked TODOs, and environment blockers.

## Explore & Reuse

- Reuse `scripts/check-spec-test-drift.sh` and its existing capability scan.
- Reuse `scripts/check-reuse.sh` and its current duplicate diagnostics.
- Reuse `scripts/coverage.sh`, `Makefile`, and `scripts/test-gates.sh`.
- Reuse `docs/TODOS.md` as the source for deliberate unfinished work.
- Reuse `openspec/governance/unprotected-baseline.txt` for ratcheted debt.

## Boundaries and data flow

The gate layer reads OpenSpec files, source manifests, and CI configuration;
it does not alter application code. New/modified capability IDs are compared
with the baseline, coverage is produced by one pinned tool, and each failure
names the exact capability, item, or evidence record.

## Verification

Fixture-driven self-tests must prove clean and violating repositories. Run
the focused gate tests, strict OpenSpec validation, then `make check` with
the configured coverage tool. Environment failures are reported separately.

## Non-goals

- Replacing Cargo, Clippy, or OpenSpec.
- Rewriting all existing specs in one change.
- Treating generated code or test-only mocks as production duplicates.
