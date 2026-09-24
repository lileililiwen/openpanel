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
`blocking:` block (the default is `FAIL` and `REVIEW_REQUIRED`).

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
  repository root and the underlying checks (the `commands:`
  bindings in `.ai-gate/gate.yaml`) all pass
- **THEN** the command exits zero, prints
  `gate plan resolved`, and writes one `gate_runs` row to
  `.driftwatch/state.db`.

#### Scenario: A `commands:` entry names a missing script

- **WHEN** `.ai-gate/gate.yaml` declares a `commands:` entry
  that names a script under `scripts/` which does not exist
- **THEN** the `driftwatchdog gate` run fails the named check,
  prints the missing path, and exits non-zero; the new self-test
  fixture in `scripts/test-gates.sh` also fails.

#### Scenario: `make check` is preserved as a developer convenience

- **WHEN** a contributor runs `make check` instead of
  `driftwatchdog gate`
- **THEN** `make check` still runs the same gate scripts and
  still exits non-zero on any failure; it remains a working
  alias for the canonical gate, just not the canonical entry
  point.

#### Scenario: The operator_security flake is resolved

- **WHEN** the `cargo test --workspace` step in the
  `driftwatchdog gate` `commands:` binding runs against the
  retuned (or quarantined) `operator_security_control_plane_suppression_expires_and_reopens`
  test
- **THEN** the step exits zero under the documented CI
  parallelism and the flake is no longer reproducible under
  `driftwatchdog gate`; the retune / quarantine decision is
  recorded in the change's `tasks.md` with a dated rationale.
