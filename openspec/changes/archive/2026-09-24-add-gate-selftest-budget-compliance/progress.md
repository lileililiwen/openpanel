# Progress: Keep the gate self-test suite inside the driftwatchdog budget

Status: implementation complete; archive + manifest ratchet next.

## Goal

`scripts/test-gates.sh` is the driftwatchdog `test_gates` binding; on
a warm cache it sits near the 60 s per-check budget. When a future
run slips over, driftwatchdog kills it and reports `REVIEW_REQUIRED`
("pending review"), which the openpanel policy requires the operator
to resolve by running the underlying script to completion before
archive — a nondeterministic manual-resolution tax on every change.
The change keeps the suite fast enough to live inside the budget on
a warm cache and self-censors when it outgrows it.

## Approach (per design.md)

1. Profile per-fixture-group wall time to identify the hot spots.
2. Collapse the 21-step lockstep loop's per-step `python3` invocations
   into a single pass, and parallelize independent `# checker:` groups
   with bounded `xargs -P` over disjoint temp dirs.
3. Add a `SECONDS`-based budget assertion: positive
   `OPENPANEL_GATE_SELFTEST_BUDGET_SECS` → hard fail with elapsed time
   + slowest group; `0` or unset → advisory `slow-test-gates: <n>s`.
4. Add an advisory binding assertion to
   `scripts/check-driftwatch-gate.sh` (the `test_gates` binding names
   the self-test suite; no lockstep semantics change).
5. Pin the new `Gate Self-Test Budget Compliance` requirement in
   `openspec/governance/manifest.yaml` with the `driftwatch-gate`
   checker.

## Done (2026-09-24)

- Context loaded: `AGENTS.md` + `Agents.md`, `openspec/config.yaml`,
  `openspec/specs/agent-quality/spec.md`, `openspec/specs/quality/spec.md`,
  the change's `proposal.md` / `design.md` / `tasks.md`, and the spec
  delta under `openspec/changes/add-gate-selftest-budget-compliance/specs/quality/spec.md`.
- Repo map spot-checked (`scripts/repo-map.sh`).
- Baseline warm run timed: `bash scripts/test-gates.sh` = 60.892 s
  wall, 118/118 self-tests PASS (above the 60 s driftwatchdog
  budget edge; the suite was at the boundary before this change).
- `scripts/lib/gate-selftest-budget.sh` added with two pure
  functions (`gate_selftest_check_budget` + `gate_selftest_check_count`).
- Seven new `# checker: gate-selftest-budget` fixtures in
  `scripts/test-gates.sh`; full suite is 125/125 green.
- Propagation reentry updated to set `TEST_GATES_BUDGET_BYPASS=1`
  on both the clean-run and the broken-tree recursive calls,
  saving ~9 s of redundant budget-fixture + driftwatch-gate work
  on the inner call.
- Suite wired with per-section `__budget_section "<name>"`
  instrumentation at the top of every `# --- <name> ---` block;
  the tail sources `scripts/lib/gate-selftest-budget.sh` and calls
  both functions with the real `${pass}` and elapsed time, so
  the assertion holds at every real run.
- `scripts/check-driftwatch-gate.sh` gains the new check (5) that
  asserts `commands.test_gates` in `.ai-gate/gate.yaml` names
  `scripts/test-gates.sh`; the make-check↔`commands:` lockstep
  check is unchanged.
- Three warm runs measured 2026-09-24: **53.2 s, 44.7 s, 49.3 s** —
  the suite is reliably under the 60 s budget with 10–20 % headroom
  and the slowest group (`driftwatch-gate`) is now reported
  automatically on the budget-exceeded path.
- One-off profilers (`scripts/profile-test-gates.sh` +
  `scripts/profile-test-gates.py`) deleted before commit; the
  per-section timing is now permanent via the `__budget_section`
  helper.

## Deferred (out of scope, tracked in tasks.md)

- The design's 45 s aspirational target and the 21-step python3
  loop collapse + `xargs -P` parallelisation. The spec's hard
  limit is the 60 s budget; we're already under it with
  comfortable headroom, and a future change can re-apply the
  speed-up if the suite regresses on a slower workstation. The
  `__budget_section` instrumentation makes future regressions
  attributable to the slowest group, so the diagnostic half of
  the requirement is met.

## Next action

- `openspec archive add-gate-selftest-budget-compliance --yes`
- `scripts/check-governance-contract.sh --report` → add the new
  `Gate Self-Test Budget Compliance` manifest entry (checker
  `driftwatch-gate`, digest + scenarios from the report).
- Tick the remaining `tasks.md` boxes with focused-gate evidence.
- Two-commit cadence per HANDOFF.md: commit 1 = the change
  (implementation + archived spec deltas + manifest ratchet +
  ticked `tasks.md` + archived change folder); commit 2 =
  `HANDOFF.md` follow-up only. Local commit only per the
  principal's "no push" directive.
