# Proposal: Add driftwatchdog-gate as the canonical Local Gate

## Why

OpenPanel's current Local Gate is the hand-rolled `make check` chain
(fmt, clippy, docs, audit, file-length, scan-literal, class-coverage,
browser-ui-quality, release-governance, release-evidence,
portable-runtime, deployment-adapters, application-delivery,
tasks-testing-first, reuse-strict, layering, spec-test-drift-strict,
spec-drift, agent-governance, governance-contract, coverage-floor,
maturity, test-gates, test). The same workspace already ships a
generic gate runtime — `driftwatchdog` (the sibling CLI at
`/home/paul/code/driftwatchdog`) — which labrys, hermora, and other
sibling projects use as their canonical Local Gate. Labrys'
`.ai-gate/gate.yaml` binds every gate to a `commands:` shell entry
and the team records `driftwatchdog gate 8/8 PASS` after every change.

OpenPanel's current `.ai-gate/gate.yaml` is a **no-op**:

```text
$ driftwatchdog gate --dry-run
…
checks:
  - build [required] via manifest
  - repository [required] via manifest
  - security [required] via manifest
  - tests [required] via manifest
project commands:
  (none)
```

`driftwatchdog gate` resolves the plan from the `rust-platform`
profile but executes no project commands because the `commands:`
block is empty. The Local Gate policy in openpanel lives in two
places (`.ai-gate/gate.yaml` and the `Makefile` `check:` target)
that drift out of sync.

The P4 change `add-git-application-delivery` was archived with a
**focused-gate waiver** of `make check` because of a pre-existing
flake
(`operator_security::operator_security_control_plane_suppression_expires_and_reopens`,
a 1200 ms `tokio::time::sleep` timing test). The waiver is
intentionally narrow and P4-only. The next change must
restore the canonical Local Gate — by replacing the hand-rolled
`make check` chain with `driftwatchdog gate` as the canonical
pre-archive gate, and re-evaluating the operator_security flake
under that runner.

## What Changes

- Rewrite `.ai-gate/gate.yaml` to bind every `make check` step to
  a `driftwatchdog` `command:` entry (the labrys pattern). Add
  `profile: rust-platform` and `rule_pack: rust-platform@0.1.0` so
  the gate plan resolves to the same set of checks. Add a new
  `application_delivery_contract` check bound to
  `scripts/check-application-delivery.sh`.
- Add a new `governance-runtime` (or `quality`) requirement
  "DriftWatchdog Local Gate" that pins:
  * the canonical pre-archive gate is `driftwatchdog gate`
    (executed in the repo root, exit non-zero on FAIL /
    REVIEW_REQUIRED);
  * `.ai-gate/gate.yaml` declares every gate that today lives in
    the `Makefile` `check:` target as a `commands:` binding;
  * `make check` is preserved as a developer convenience and CI
    mirror, but is **not** the canonical pre-archive gate;
  * the operator_security flake (and any other pre-existing flake
    surfaced under the new runner) is either retuned (preferred)
    or quarantined with a documented reason.
- Add `make gate` as a thin wrapper around `driftwatchdog gate`
  so the developer experience matches the existing
  `make <gate>` style.
- Add a `scripts/check-driftwatch-gate.sh` self-test fixture that
  asserts `.ai-gate/gate.yaml` is well-formed, the `commands:`
  block names every step that today lives in the `Makefile` `check:`
  target, and the plan resolves cleanly under `driftwatchdog gate
  --dry-run`.
- Update `AGENTS.md` "Quality gate" line to declare
  `driftwatchdog gate` as the canonical Local Gate and `make check`
  as a developer convenience. Update `HANDOFF.md` "Required
  execution protocol" to match.
- The `driftwatchdog` binary is already installed at
  `~/.cargo/bin/driftwatchdog` (verified by `driftwatchdog doctor`
  on 2026-09-24) and the workspace has a `.driftwatch/` directory
  the runtime expects. No new dependency is introduced; the
  migration is a policy change that reuses an existing binary.

## BFS Impact Map

- Capabilities: `quality` is modified (the new DriftWatchdog Local
  Gate requirement); a new `governance-runtime` capability may be
  added if the team prefers a separate spec over folding the
  requirement into `quality`.
- Users: every contributor who runs the pre-archive gate; the CI
  pipeline (which will read the same `.ai-gate/gate.yaml` and call
  `driftwatchdog gate` the same way).
- Contracts/data: `.ai-gate/gate.yaml` (canonical policy),
  `Makefile` (developer convenience), `AGENTS.md` (the
  one-source-of-truth "Quality gate" line), `HANDOFF.md` (the
  protocol), and the existing gate scripts under `scripts/`
  (unchanged — they remain the executable checks the
  `commands:` block calls).
- Integrations: `driftwatchdog` CLI (`~/.cargo/bin/driftwatchdog`,
  present in the workspace as a sibling project, also a runtime
  in CI). The `.driftwatch/state.db` is the persistence record
  `driftwatchdog gate` uses to track gate-run history.
- Failure boundaries: driftwatchdog binary missing (recoverable —
  fall back to `make check` for that one run with a documented
  warning), `.ai-gate/gate.yaml` malformed (fail-closed via the
  new self-test fixture), a `commands:` entry naming a script that
  does not exist (fail-closed the same way), a step that exits
  non-zero (gate exits non-zero, the change is not archive-ready).
- Unaffected: every production binary, every HTTP route, every
  CLI subcommand, every spec, every existing test, every existing
  gate script. The migration is a tools-and-infrastructure change
  that strengthens the pre-archive gate without altering the
  panel.

## Capabilities

### Modified Capabilities

- `quality`

### New Capabilities (optional)

- `governance-runtime` — a one-requirement spec that pins
  `driftwatchdog gate` as the canonical Local Gate and the
  `commands:` block as the canonical policy surface.

## Non-goals

- No replacement of any existing gate script. The
  `scripts/check-*.sh` files stay; the migration only changes
  which process invokes them.
- No claim that driftwatchdog is a CI replacement. The CI
  pipeline keeps its own `make check` job (or migrates to
  `driftwatchdog gate` separately) — the two are independent
  decisions and the migration does not couple them.
- No new dependency on a network or remote service. The
  `driftwatchdog` CLI is local-first, writes to
  `.driftwatch/state.db`, and the gate is fully offline.
- No new policy without a human-principal-approved `design.md`
  that records the exact `commands:` table, the new requirement
  wording, the gate-step ordering, and the failure semantics.
  This proposal is the **why**; the design phase is the **what**.

## Open questions for the design phase

1. **Profile choice**: `rust-platform` (labrys) or
   `rust-product` (driftwatchdog's native Rust profile). The two
   surface different checks; the design must pick one and
   justify the choice in `design.md`.
2. **Quarantine vs retune**: the pre-existing
   `operator_security::operator_security_control_plane_suppression_expires_and_reopens`
   flake. The design should propose either (a) lengthening the
   `tokio::time::sleep` to 2000 ms with a comment, (b) replacing
   the timing assertion with a deterministic poll loop, or (c)
   a documented `#[ignore = "<reason>"]` quarantine.
3. **`make check` semantics**: keep `make check` as a thin wrapper
   that calls `driftwatchdog gate` (so `make check` IS the gate),
   or keep `make check` as a parallel convenience that runs the
   same steps independently. The design must pick one.
4. **CI mirror**: does `.github/workflows/ci.yml` change in this
   change, or stay on `make check` and migrate in a follow-up?
   The design should be explicit.
