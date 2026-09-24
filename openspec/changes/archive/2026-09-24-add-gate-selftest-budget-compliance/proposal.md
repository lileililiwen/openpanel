# Proposal: Keep the gate self-test suite inside the driftwatchdog budget

## Why

`driftwatchdog gate` enforces a hard-coded 60 s per-check budget
(0.1.0, no per-command override). Per-check wall timings measured
2026-09-24 on a warm cache: `test_gates` = 65 s, `tests` = 34 s,
`spec_test_drift_strict` = 18 s, `reuse_strict` = 11 s, all other
bindings ≤ 2 s. Only `test_gates` sits at the budget edge, so it
flaps between PASS (idle runs) and a "pending review" (loaded runs) —
a nondeterministic manual-resolution tax on every future change before
archive. The live `DriftWatchdog Local Gate` requirement deliberately
tolerates pending reviews for cold cargo builds and MUST NOT be edited
(digest-pinned in `openspec/governance/manifest.yaml`); this change
instead fixes the offender and pins the new guarantee as a standalone
requirement. No existing logic satisfies this: `test-gates.sh` has no
timing assertion, and `check-tests-gate.sh` (P5) established the
budget-sizing convention only for the `tests` binding.

## What Changes

- Speed `scripts/test-gates.sh` to a reliable sub-60 s warm run:
  profile per-fixture-group wall time first, then parallelize
  independent fixture groups and collapse the 21-step lockstep loop's
  per-step `python3` invocations into one.
- Add a self-enforcing budget assertion inside `test-gates.sh`: the
  suite fails with its elapsed wall time when a warm run exceeds the
  configured budget (overridable via env for cold CI).
- New `quality` requirement `Gate Self-Test Budget Compliance`, pinned
  in the governance manifest at archive.

## BFS Impact Map

- Capability: `quality` (delta: one ADDED requirement; no existing
  requirement text touched).
- Affected files: `scripts/test-gates.sh` (runtime + assertion),
  `scripts/check-driftwatch-gate.sh` (advisory: surface the budget
  binding), `openspec/governance/manifest.yaml` (new pin at archive).
- Consumers: every change's pre-archive `driftwatchdog gate` run;
  `make test-gates` and its `check:`-chain binding (names unchanged,
  lockstep preserved); CI (same script, cold-cache path handled by the
  env override).
- Unaffected: product crates, `.ai-gate/gate.yaml` command bindings
  (no step added or renamed), protected `DriftWatchdog Local Gate`
  text, P7 folder.
- Failure behavior: budget exceeded warm → suite exits non-zero with
  elapsed time + slowest group; cold CI → non-blocking advisory line.

## Capabilities

- Modified: `quality` — source-of-truth spec
  `openspec/specs/quality/spec.md`; delta adds one requirement only.

## Non-goals

- No driftwatchdog fork/pin bump to make the budget configurable.
- No reduction or removal of fixtures (coverage may only grow or stay).
- No speed-up of cargo-native bindings (`tests`, `docs`, `build`) —
  cold cargo runtime is legitimately over-budget by current policy.
- No change to the protected `DriftWatchdog Local Gate` requirement.
