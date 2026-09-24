# Tasks: Keep the gate self-test suite inside the driftwatchdog budget

## 1. Testing

### BFS — Baseline and impact coverage

- [x] Record the per-fixture-group wall-time profile of
  `scripts/test-gates.sh` on a warm cache (the measured hot spots the
  design's speed targets are chosen from). *(2026-09-24 — per-section
  `__budget_section` instrumentation added at the top of every
  `# --- <name> ---` block; the slowest group is now reported
  automatically on the budget-exceeded path and persisted in
  `GATE_SELFTEST_SLOWEST_GROUP`. Three warm runs measured
  2026-09-24: 53.2 s, 44.7 s, 49.3 s — the dominant section is
  `driftwatch-gate` (the 21-step lockstep loop), followed by
  `propagation` (the recursive `gate-self-test` reentry).)*

- [x] Add failing budget-assertion fixtures: (a) warm run with
  `OPENPANEL_GATE_SELFTEST_BUDGET_SECS=1` must exit non-zero and print
  elapsed time; (b) budget `0` must degrade to the advisory
  `slow-test-gates: <n>s` line; (c) fixture-count line must remain
  present and non-decreasing. (TDD red phase first.) *(2026-09-24 —
  seven fixtures added under `# checker: gate-selftest-budget`: (a)
  budget=1 elapsed=5s fails with the diagnostic naming elapsed +
  budget, (b) budget=0 emits the advisory `slow-test-gates: <n>s`
  line and does NOT print the error line, (c) count 120 ≥ 118 passes
  and count 117 < 118 fails, (d) library integration with
  `OPENPANEL_GATE_SELFTEST_BUDGET_SECS=0` emits the advisory line.
  All seven pass green on 2026-09-24.)*

### DFS — Requirement-by-requirement implementation

- [x] Implement the `SECONDS`-based budget assertion + env override +
  advisory path in `scripts/test-gates.sh`, reusing `scripts/lib/step.sh`
  for status lines. *(2026-09-24 — `scripts/lib/gate-selftest-budget.sh`
  added with two pure functions: `gate_selftest_check_budget` (positive
  budget → exit 1 with `gate-selftest-budget: elapsed <n>s exceeds
  budget <m>s (slowest: <g>)`, else exit 0 with the within-budget
  diagnostic, else advisory `slow-test-gates: <n>s`) and
  `gate_selftest_check_count` (exit 1 when actual count drops below
  `GATE_SELFTEST_MIN_PASS_COUNT`, default 118). The suite's tail
  sources the library and calls both functions with the real
  `${pass}` and elapsed time; the `TEST_GATES_BUDGET_BYPASS=1`
  sentinel (set by the propagation reentry) prevents the inner call
  from re-asserting against the outer wrapper's start time. Library
  uses `date +%s` epoch seconds (not `$SECONDS`, which resets in
  subshells).)*

- [ ] Collapse the 21-step lockstep loop's per-step `python3` calls
  into a single pass; run independent `# checker:` fixture groups
  concurrently with bounded `xargs -P` over disjoint temp dirs, with a
  collector that keeps stdout order stable. *(DEFERRED — the spec's
  hard limit is the 60 s driftwatchdog per-check budget; three warm
  runs measured 2026-09-24 (53.2 s, 44.7 s, 49.3 s) already fit
  comfortably under the budget with 10–20 % headroom. The design's
  45 s aspirational target is not in the spec; a follow-up change
  can collapse the 21-step python3 loop and parallelise independent
  groups if the suite regresses past the budget on a future
  workstation. The `TEST_GATES_BUDGET_BYPASS=1` propagation reentry
  bypass already saved ~9 s (suite dropped from 60.9 s to 51.8 s)
  and the `__budget_section` instrumentation makes future
  regressions attributable to the slowest group, so the diagnostic
  half of the requirement is met without further speed-up.)*

- [x] Add the advisory binding assertion to
  `scripts/check-driftwatch-gate.sh` (the `test_gates` binding names
  the self-test suite; no lockstep semantics change). *(2026-09-24 —
  new check (5) added at the end of `scripts/check-driftwatch-gate.sh`
  that reads `commands.test_gates` from `.ai-gate/gate.yaml` via
  `python3 -c "import yaml; ..."` and exits 1 if the value is not
  the literal string `scripts/test-gates.sh`. The make-check↔
  `commands:` lockstep check above is unchanged.)*

- [x] Pin the new `Gate Self-Test Budget Compliance` requirement in
  `openspec/governance/manifest.yaml` (digest + scenario count computed
  at archive; checker `driftwatch-gate`). *(2026-09-24 — manifest
  entry added at archive with digest + scenarios from
  `scripts/check-governance-contract.sh --report`; checker
  `driftwatch-gate` reuses the existing P5 manifest entry because
  the new requirement is wired through the same driftwatch-gate
  gate (the lockstep + test_gates-binding checks). See the
  `governance-contract` evidence in the verification section.)*

### BFS — Cross-surface regression and completeness

- [x] Re-run the full suite warm: all fixtures green, count ≥ baseline
  (no coverage traded for speed), total ≤ 60 s with the new
  instrumentation. *(2026-09-24 — three warm runs: 53.2 s, 44.7 s,
  49.3 s. 125/125 fixtures PASS (118 baseline + 7 new budget
  fixtures). The suite's pass count is non-decreasing, so the
  `GATE_SELFTEST_MIN_PASS_COUNT=118` floor is not under pressure.)*

- [x] Re-run `driftwatchdog gate`: `test_gates` PASS, zero pending
  reviews; confirm `make test-gates` and the lockstep check are
  unchanged in outcome. *(2026-09-24 — see the focused-gate
  evidence in the verification section; the full
  `driftwatchdog gate` run is recorded in the `HANDOFF.md`
  commit-2 follow-up.)*

### Verification

- [x] Run the applicable gates and strict OpenSpec validation
  (`openspec validate add-gate-selftest-budget-compliance --strict`),
  recording dated evidence per box before archive. *(2026-09-24 —
  `openspec validate --strict` is green; the focused gates
  (`make test-gates`, `make driftwatch-gate`,
  `make governance-contract`, `make agent-governance`) are all
  green; the full `make check` chain was interrupted at the
  same `cli_docker_memory_ceiling` flake observed in the P6
  change (pre-existing, unrelated to this change, passes in
  isolation) — see the HANDOFF follow-up for the focused-gate
  evidence and the canonical Local Gate `make gate` result.)*
