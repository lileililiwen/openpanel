# Tasks: Add driftwatchdog-gate as the canonical Local Gate

## 1. Testing

### BFS — Baseline and impact coverage

- [ ] Map every step in the current `Makefile` `check:` target
  to a corresponding `commands:` entry under
  `.ai-gate/gate.yaml`. Confirm one-to-one coverage before
  declaring the migration complete; the set MUST include
  `repository_integrity`, `format`, `lint`, `build`, `tests`,
  `security`, `audit`, `file-length`, `scan-literal`,
  `class-coverage`, `browser-ui-quality`, `release-governance`,
  `release-evidence`, `portable-runtime`, `deployment-adapters`,
  `application-delivery`, `tasks-testing-first`, `reuse` (strict),
  `layering`, `spec-test-drift` (strict), `spec-drift`,
  `agent-governance`, `governance-contract`, `coverage-floor`,
  `maturity`, `test-gates`.
- [ ] Verify the existing `scripts/test-gates.sh` fixtures still
  pass under the new gate runner; the new self-test fixture
  `scripts/check-driftwatch-gate.sh` (or equivalent) MUST add
  positive and negative coverage for `.ai-gate/gate.yaml`
  well-formedness, `commands:`-name resolution, and a
  `driftwatchdog gate --dry-run` plan resolution.

### DFS — Requirement-by-requirement implementation

- [ ] Rewrite `.ai-gate/gate.yaml` to declare
  `profile: rust-platform`, `rule_pack: rust-platform@0.1.0`
  (or the design-approved alternative), every required
  `checks:` entry, and a `commands:` block that binds each
  check to a real shell command. The shape MUST follow the
  labrys precedent.
- [ ] Add `make gate` as a thin wrapper around
  `driftwatchdog gate`. Confirm `make -n gate` prints
  `driftwatchdog gate` and that `make gate` exits zero on a
  clean tree.
- [ ] Add a `gate` step (and a `gate_runs` row in
  `.driftwatch/state.db`) to the pre-archive workflow.
  Confirm the pre-archive `make check` (developer convenience)
  and the new `make gate` (canonical) both pass on a clean
  tree.
- [ ] Re-evaluate the pre-existing
  `operator_security::operator_security_control_plane_suppression_expires_and_reopens`
  flake. Decide retune vs quarantine per the design;
  implement the chosen path; document the decision and date
  in this `tasks.md`.
- [ ] Update `AGENTS.md` "Quality gate" line to declare
  `driftwatchdog gate` as the canonical Local Gate and
  `make check` as a developer convenience. Update
  `HANDOFF.md` "Required execution protocol" and
  "Definition of done per change" to match.

### BFS — Cross-surface regression and completeness

- [ ] Confirm no production code, no HTTP route, no CLI
  subcommand, no test, and no spec is changed by the
  migration. The diff MUST be limited to `.ai-gate/gate.yaml`,
  the `Makefile` (additions only), the new self-test fixture,
  `AGENTS.md`, `HANDOFF.md`, and this `tasks.md`.
- [ ] Confirm CI behaviour. Decide (in the design) whether
  `.github/workflows/ci.yml` migrates in this change or
  stays on `make check` and migrates in a follow-up.

### Verification

- [ ] Run `driftwatchdog gate` in the repository root and
  record the result in `HANDOFF.md` (e.g.
  `driftwatchdog gate 8/8 PASS` per the labrys convention,
  adjusted to the actual number of checks the design picks).
- [ ] Run `openspec validate add-driftwatchdog-gate-migration --strict`
  and confirm zero failures.
- [ ] Run the full `make check` chain once on a clean tree
  and confirm it still passes (this is a regression check,
  not the canonical pre-archive gate).
