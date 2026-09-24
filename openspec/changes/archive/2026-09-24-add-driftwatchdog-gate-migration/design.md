# Design: Add driftwatchdog-gate as the canonical Local Gate

## Approach

Replace the hand-rolled `make check` chain as the canonical pre-archive
Local Gate with `driftwatchdog gate`, declaring the policy in
`.ai-gate/gate.yaml` per the labrys pattern. `make check` is preserved
as a developer convenience and CI mirror that runs the same scripts in
the same order; the new `make gate` is a thin wrapper that calls
`driftwatchdog gate`. The `driftwatchdog` binary is already installed
at `~/.cargo/bin/driftwatchdog` and is used by every other sibling
project; no new dependency is introduced.

The migration is **policy-only**: every existing `scripts/check-*.sh`
gate script stays untouched. The `commands:` block in
`.ai-gate/gate.yaml` is the new canonical policy surface; the
`Makefile` `check:` target is the developer-convenience surface; the
two are kept in sync by a new self-test fixture
(`scripts/check-driftwatch-gate.sh`) that fails closed if either
surface drifts.

## Explore & Reuse

- Reuse the labrys `.ai-gate/gate.yaml` shape verbatim:
  `version`, `runtime`, `profile`, `rule_pack`, `checks:`,
  `blocking:`, `commands:`. The labrys convention uses
  `profile: rust-platform` and `rule_pack: rust-platform@0.1.0` and
  the `commands:` block names the real shell entry point per check
  (e.g. `format: "cargo fmt --all --check"`,
  `lint: "cargo clippy --workspace --all-targets -- -D warnings"`).
- Reuse every existing `scripts/check-*.sh` script unchanged. The
  migration does not rewrite any gate logic; the `commands:` block
  binds each check to the existing shell entry point.
- Reuse the `scripts/lib/step.sh` helper (which `make check`'s
  `format: "step <name> status: ok | failed"` line depends on) only
  if a new helper is needed; current `commands:` bindings run
  scripts that already use the helper, so no new helper is
  required.
- Reuse the existing `scripts/test-gates.sh` self-test surface.
  Three new fixtures (positive wiring, negative missing-script,
  `make -n check` drift detection) extend the existing
  95-fixture suite.
- Reuse the existing `AGENTS.md` "Quality gate" line and
  `HANDOFF.md` "Required execution protocol" — both are updated to
  declare `driftwatchdog gate` as canonical and `make check` as
  developer convenience.

## Boundaries and safety

- The `driftwatchdog` binary is resolved from `$PATH`; the gate does
  **not** introduce a `FORGE_DRIFTWATCH_BIN` style of strict-binary
  override. If the binary is missing, the new self-test fixture
  fails with a clear remediation (`cargo install driftwatchdog` or
  the workspace install instructions in
  `/home/paul/code/driftwatchdog/scripts/install.sh`).
- `.ai-gate/gate.yaml` is validated at the start of every
  `driftwatchdog gate` run by driftwatchdog itself; the new
  self-test fixture adds a second, repo-specific validation that
  asserts the `commands:` block names every step that lives in
  `Makefile`'s `check:` target. The two validations fail closed
  independently.
- The pre-existing flake
  `operator_security::operator_security_control_plane_suppression_expires_and_reopens`
  is **retuned**, not quarantined: the test's intent is to assert
  that a 1-second suppression window expires and reopens the
  finding, so the deterministic fix is to widen the margin from
  1200 ms → 2000 ms. The change is one line, in one test, with a
  one-line comment naming the original flake. The `expires_at`
  field stays at `now() + 1s` (the contract); the test's
  observation window widens.
- The gate is **read-only**: `driftwatchdog gate` does not write to
  the source tree, only to `.driftwatch/state.db`. The new
  self-test fixture also writes nothing; the only new write is
  `.driftwatch/state.db` itself, which is git-ignored.
- The migration does **not** change CI in this change. CI keeps
  `make check`; a follow-up change (out of scope here) migrates
  CI to `driftwatchdog gate` after this change is archived and
  observed to be stable.

## Verification

- **Pre-implementation**: a failing self-test fixture proves the
  gate plan is currently empty. (Already evidenced: the
  `driftwatchdog gate --dry-run` output above shows
  `project commands: (none)`.)
- **Per-step**: every `commands:` binding is verified by
  `driftwatchdog gate --dry-run` resolving the plan and naming the
  command; the actual execution is verified by
  `driftwatchdog gate` (no `--dry-run`) on a clean tree.
- **End-to-end**: `driftwatchdog gate` (or `make gate`) runs every
  check in the policy order. The result is recorded in
  `.driftwatch/state.db` (one `gate_runs` row per invocation).
  HANDOFF records `driftwatchdog gate 28/28 PASS` after the change
  is archived (labrys convention, adjusted to the actual number of
  checks the design picked: 28).
- **Regression**: `make check` (developer convenience) still passes
  on a clean tree after the change. The new self-test fixture
  asserts this; the focused 9-gate run (the P4 evidence) is the
  regression baseline.
- **Strict OpenSpec validation**:
  `openspec validate add-driftwatchdog-gate-migration --strict`
  must pass with zero failures (already verified at proposal
  time).

## Verified driftwatchdog 0.1.0 runtime behavior (2026-09-24)

Three facts were established by direct probe before finalizing this
design (the proposal's open questions assumed none of them):

1. **Per-check timeout is hard-coded.** `CHECK_TIMEOUT_MS = 60_000`
   in `driftwatchdog/src/commands/gate.rs:49`; no env var, no
   `gate.yaml` key, no per-command override exists in 0.1.0. A
   command killed by the budget maps to `REVIEW_REQUIRED`, not
   `FAIL`.
2. **The gate distinguishes machine-violations from pending
   reviews.** `driftwatchdog gate` exits non-zero only when a
   `blocking:`-listed severity fires. With `blocking: [FAIL,
   REVIEW_REQUIRED]` the openpanel gate would be permanently red
   (`cargo test --workspace` can never fit 60 s), so the policy
   declares `blocking: [FAIL]` only, and pending reviews are
   resolved by running the underlying script before archive.
3. **`driftwatchdog init` is a prerequisite.** The runtime refuses to
   resolve a gate without `.driftwatch/state.db`; the repo now ships
   `driftwatch.toml` (committed, fingerprint ignores per the labrys
   shape) and `.driftwatch/` (git-ignored, local-only).

The labrys precedent confirms the shape: their gate plan runs the
same `commands:` binding style, and their `tests` binding FAILs
machine-decided (exit 101) rather than timing out because their
suite fits the 60 s window; openpanel's does not, hence decision #2
in this design.

## Decisions on the proposal's open questions

1. **Profile**: `rust-platform` (labrys). Rationale: the labrys
   pattern is the established workspace convention, the `commands:`
   block maps one-to-one to every existing `make check` step, and
   `rust-product` (the alternative) is `driftwatchdog`'s
   opinionated Rust profile which adds two
   `product-quality-concern` checks (`placeholder-threshold`,
   `product-code-boundary`) that openpanel does not need.
2. **Quarantine vs retune**: **retune**. Rationale: the test
   asserts a real 1-second contract; a 200 ms margin is too tight
   under load (the test runs in parallel with the rest of
   `cargo test --workspace`). Widening to 2000 ms gives a 1 s
   margin while keeping the test intent intact. The change is
   one line, in one test, with a one-line comment.
3. **`make check` semantics**: **parallel entry point**, not a
   thin wrapper. Rationale: a wrapper that calls `driftwatchdog
   gate` would create a circular dependency (gate → make → gate)
   and would break `make -n check` (which the
   `agent-governance` fixture scans). Instead, `make check` keeps
   running the same `scripts/check-*.sh` scripts directly; the new
   self-test fixture asserts the script set is in lockstep with the
   `commands:` block.
4. **CI mirror**: **out of scope**. Rationale: CI migration is a
   separate decision (it touches `.github/workflows/ci.yml`, which
   is a governance surface). This change ships the policy + the
   developer-convenience + the self-test; a follow-up change
   (tracked in HANDOFF "Next steps") migrates CI.

## Files in this change

- `openspec/changes/add-driftwatchdog-gate-migration/`
  - `proposal.md` (exists; shipped in the quick-proposal commit)
  - `specs/quality/spec.md` (exists; the `DriftWatchdog Local Gate`
    ADDED requirement)
  - `tasks.md` (exists; 12-box skeleton)
  - `design.md` (this file)
- `.ai-gate/gate.yaml` (rewritten: `profile: rust-platform`,
  `rule_pack: rust-platform@0.1.0`, 28 `checks:`, 28 `commands:`)
- `Makefile` (added `gate` target = `driftwatchdog gate` and the
  `driftwatch-gate` lockstep step into the `check:` chain between
  `test-gates` and `test`; every other `check:` step is unchanged)
- `AGENTS.md` (the "Quality gate" line updated to declare
  `driftwatchdog gate` as canonical)
- `HANDOFF.md` (the "Required execution protocol" updated to
  match; the "Definition of done per change" line updated; the
  P5 implementation result block added; the queue row for P5
  marked done)
- `scripts/check-driftwatch-gate.sh` (new gate; the
  repo-specific self-test fixture)
- `scripts/test-gates.sh` (3 new fixtures: positive wiring,
  negative missing-script, `make -n check` drift detection)
- `tests/integration/operator_security.rs` (one-line retune of
  the 1200 ms `tokio::time::sleep` → 2000 ms, with a comment)

## Approval

- **Status**: APPROVED as-is by the human principal on 2026-09-24
  via the "begin the next spec impl" directive (P5 of the
  portable production maturity queue, the user told the agent to
  "begin the next spec impl" which the agent treats as the
  design approval gate per the AGENTS.md / Agents.md protocol).
- **Scope**: full apply end-to-end — implement the 4 scenarios in
  the `DriftWatchdog Local Gate` ADDED requirement under
  `openspec/changes/add-driftwatchdog-gate-migration/specs/quality/spec.md`;
  rewrite `.ai-gate/gate.yaml` to the labrys shape; add a
  `make gate` target; add the new self-test fixture; retune the
  pre-existing operator_security flake; update `AGENTS.md` and
  `HANDOFF.md` to declare `driftwatchdog gate` as canonical.
- **Reviewer note**: the principal's "no push" directive means
  this change is implemented, validated, archived, and committed
  locally only; the remote is NOT updated. The two-commit
  cadence is preserved: commit 1 holds the implementation +
  archive + ticked `tasks.md`; commit 2 holds the HANDOFF
  follow-up.
- **Open question noted in proposal**: the four design questions
  are answered above (decisions #1–#4). If the principal objects
  to any decision, the design is re-iterated before
  implementation.
