# Design: Keep the gate self-test suite inside the driftwatchdog budget

## Approach

Two layers, both inside the governance-script surface:

1. **Speed.** Profile `scripts/test-gates.sh` by fixture group
   (`time` around each `# checker:` block), then eliminate the measured
   hot spots: run independent fixture groups concurrently (bounded
   `xargs -P` over groups that own disjoint temp dirs), and collapse
   the 21-step `commands:`-lockstep loop's per-step `python3`
   invocations into a single pass. Target: ≤ 45 s warm (25 % headroom
   under the 60 s budget).
2. **Self-enforcement.** The suite records `START=$SECONDS` and, before
   the final summary, fails when `SECONDS - START` exceeds
   `OPENPANEL_GATE_SELFTEST_BUDGET_SECS` (default `60`). When the env
   var is set to `0` (cold CI default) the assertion degrades to an
   advisory `slow-test-gates: <n>s` line — mirroring how
   `check-portable-runtime.sh` separates advisory from required.
   The suite prints elapsed time and the slowest fixture group on both
   paths so regressions are attributable.

## Explore & Reuse

- P5 `scripts/check-tests-gate.sh`: the established pattern of sizing a
  gate binding to be "machine-decidable inside the 60 s budget" — this
  change extends the same convention to `test_gates`.
- `scripts/lib/step.sh` (`step` helper): reuse for the ok/failed and
  advisory status lines — no new output format.
- `scripts/test-gates.sh`'s existing fixture conventions
  (`# checker: <id> positive|negative`) and temp-dir isolation: the
  parallelization groups fixtures by checker id only where their
  temp dirs are disjoint — no fixture-rewriting framework.
- Advisory/required split from `scripts/check-portable-runtime.sh`
  (`OPENPANEL_*_REQUIRED` precedent) for the env-gated hard failure.
- `scripts/check-driftwatch-gate.sh`: already asserts binding lockstep;
  gains only an advisory assertion that the `test_gates` binding names
  the self-test suite (no lockstep changes).

## Boundaries and safety

- Governance surface only: no product crate, no `.ai-gate/gate.yaml`
  command edits, no `Makefile` step renames — the
  make-check↔`commands:` lockcheck stays green by construction.
- The protected `DriftWatchdog Local Gate` requirement text is not
  touched; the new requirement stands alone, so
  `openspec/governance/manifest.yaml` only gains a pin (digest computed
  at archive), never a digest edit.
- Coverage floor: the ADDED requirement forbids trading fixtures for
  speed; the suite's final line must still report the fixture count
  and the self-test count (today 118) may not decrease without a
  reviewed spec change.
- Determinism: parallel groups must write results to per-group temp
  files and a single collector serializes the final output, so stdout
  stays in a stable order for the driftwatchdog summary.

## Verification

- Red first: a fixture (or temporary sleep injection) proving the
  budget assertion fails the suite warm and stays advisory with the
  env override — evidence in tasks.
- `bash scripts/test-gates.sh` warm ≤ 45 s with all fixtures
  green (count reported, none dropped).
- `driftwatchdog gate` on a clean tree: `test_gates` PASS with the run
  listed under zero pending reviews (recorded in tasks evidence).
- `scripts/check-driftwatch-gate.sh` and `make test-gates` unchanged in
  outcome.

## Open questions

- None material to scope; the concurrency bound (default `-P 4`) is an
  implementation detail to be tuned against the profile.
