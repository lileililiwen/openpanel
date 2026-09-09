# Progress: ratchet quality and spec maturity

Status: in progress (implementation)

## Goal

Tighten the four governance gates that still allow warning-only or
informational behaviour (spec-test drift, coverage, reuse, evidence of
incomplete work) and require every newly introduced gap to be classified
in a reviewed manifest.

## Approach (per `design.md`)

- Extend the existing shell gates; no second quality framework.
- Add a `ratchet-style` baseline to `check-spec-test-drift.sh` so a
  newly added capability without a covering test fails, while the
  pre-existing uncovered set is reported without failing (and only
  shrinks over time).
- Add a configured floor + tool-missing detection to `coverage.sh`,
  plus a new `make coverage-check` target that CI runs in a required
  job (not as `continue-on-error: true`).
- Add a classified-duplicate baseline to `check-reuse.sh`; `--strict`
  fails only on **unclassified** new duplicates, so the mandatory
  chain can adopt `--strict` without a retroactive mass-fail.
- Add an `openspec/governance/evidence.yaml` manifest plus
  `scripts/check-maturity.sh` that classifies TODO markers, accepted
  test fakes, and environment-blocked checks; new production stubs
  without an evidence entry fail the build.
- Add positive + negative fixtures for every new gate in
  `scripts/test-gates.sh`; register them with `# checker:` markers
  so `check-governance-contract` can verify the executable
  protection.

## Reuse

- `scripts/check-spec-test-drift.sh` — extend, do not replace.
- `scripts/coverage.sh` — extend, do not replace.
- `scripts/check-reuse.sh` — extend, do not replace.
- `scripts/lib/step.sh` — standard `step: ... status: ...` helper.
- `scripts/test-gates.sh` — checker registry.
- `openspec/governance/manifest.yaml` — pattern for reviewed entries.
- `openspec/governance/unprotected-baseline.txt` — pattern for
  tracked debt.
- `docs/TODOS.md` — first evidence record (ACME HTTP-01).
- `Makefile` — extend `check` chain.

## Phases

1. Research — done.
2. Plan — done (`proposal.md`, `design.md`, `tasks.md`); approved.
3. Implement — in progress.
4. Verify — pending.
5. Archive + commit cadence — pending.
