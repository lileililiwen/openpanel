# quality Specification

## Purpose

Quality gates that protect the OpenPanel delivery pipeline. The
`add-driftwatchdog-gate-migration` change adds a new requirement
that pins `driftwatchdog gate` as the canonical Local Gate (the
gate that every change must run before archive), with the
`.ai-gate/gate.yaml` `commands:` block as the canonical policy
surface. The hand-rolled `make check` chain stays as a developer
convenience and CI mirror; it is no longer the canonical pre-
archive gate.

## Requirements

## ADDED Requirements

### Requirement: DriftWatchdog Local Gate

The canonical pre-archive gate for every change in this repository
SHALL be `driftwatchdog gate`, executed in the repository root.

The gate policy SHALL be declared in `.ai-gate/gate.yaml` and
MUST include a `commands:` block that binds every check that
today appears in the `Makefile` `check:` target to a real shell
command. The `driftwatchdog` binary MUST be the one resolved by
`which driftwatchdog` from the developer's `PATH`; the
`FORGE_DRIFTWATCH_BIN` style of strict-binary override does not
apply to openpanel.

The gate MUST exit non-zero on any blocking failure. The set of
blocking severities is declared by `.ai-gate/gate.yaml`'s
`blocking:` block. driftwatchdog 0.1.0 hard-codes a 60 s per-check
budget; commands killed by that budget map to `REVIEW_REQUIRED`
("pending reviews"), and the openpanel policy deliberately declares
only `FAIL` as blocking because the full test suite, a cold
`cargo doc`, and `cargo audit` legitimately need longer than 60 s.
A pending review is resolved by running the underlying script
(`make test`, `make docs`, `make audit`) to completion before
archive; a change with an unresolved pending review MUST NOT be
represented as complete.

`make check` MAY be retained as a developer convenience and a CI
mirror, but it is **not** the canonical pre-archive gate. A
change that runs only `make check` and skips `driftwatchdog gate`
does not satisfy this requirement.

The pre-existing flake
`operator_security::operator_security_control_plane_suppression_expires_and_reopens`
(observed under the `make check` chain in 2026-09-24) MUST be
either retuned (preferred) or quarantined under the new runner
with a documented reason recorded in the change's `tasks.md`.

#### Scenario: `driftwatchdog gate` passes on a clean tree

- **WHEN** a contributor runs `driftwatchdog gate` in the
  repository root and no underlying check exits non-zero inside its
  60 s budget
- **THEN** the command exits zero and writes one `gate_runs` row to
  `.driftwatch/state.db`; checks killed by the 60 s budget are
  printed under "Pending reviews" and MUST be resolved by running
  the underlying script (`make test`, `make docs`, `make audit`)
  before archive.

#### Scenario: A `commands:` entry is dropped from the policy

- **WHEN** `.ai-gate/gate.yaml` is edited so a `make check` step
  (e.g. `file-length` → `file_length`) has no `commands:` binding
  while remaining in the `Makefile` `check:` target
- **THEN** `make driftwatch-gate` exits non-zero and names the
  drifted step, and `driftwatchdog gate` reports the check as
  not-executed; `scripts/test-gates.sh` exercises this with the
  `driftwatch-gate: missing commands entry fails` fixture.

#### Scenario: `make check` is preserved as a developer convenience

- **WHEN** a contributor runs `make check` instead of
  `driftwatchdog gate`
- **THEN** `make check` still runs the same gate scripts and
  still exits non-zero on any failure; it remains a working
  alias for the canonical gate, just not the canonical entry
  point.

#### Scenario: The operator_security flake is resolved

- **WHEN** the `tests` command's underlying script
  (`scripts/check-tests.sh`, i.e. `make test`) runs
  `operator_security_control_plane_suppression_expires_and_reopens`
  under full-suite parallelism
- **THEN** the retuned 2000 ms observation window makes the step
  exit 0 and the flake no longer reproduces; the driftwatchdog
  `tests` pending review (the command is killed by the 60 s budget
  before the suite can report) is resolved by that run before
  archive.
