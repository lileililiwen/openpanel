# quality Specification

## ADDED Requirements

### Requirement: Reuse / Duplication Gate

`make check` SHALL run `scripts/check-reuse.sh`, which detects a public
item defined in more than one crate (excluding `#[cfg(test)]` modules).

Function-name equality across crates is a weak signal on its own
(`router()`, `register()`, `iter()` recur per module), so the gate
**reports** any cross-crate duplicate as a **warning** by default and
does NOT fail the build. Under `scripts/check-reuse.sh --strict` (CI for
new changes, and local pre-commit) a cross-crate duplicate `pub fn`
(outside a trivial-name denylist) FAILS, forcing the author to reuse
the existing implementation instead of introducing a second one.

This directly addresses the "failure to reuse existing logic" failure
mode as a mechanical backstop; the primary defense is the Explore &
Reuse workflow step in `Agents.md`. The script MUST degrade to
`status: skipped` when `rg` is unavailable.

#### Scenario: Same function defined twice (strict)

- **WHEN** `pub fn hash_password` exists in both `openpanel-core` and
  `openpanel-app` and the gate runs in `--strict` mode
- **THEN** the step fails naming both crates, and the author must
  delete one and reuse the other.

#### Scenario: Trivial function name is not flagged

- **WHEN** `pub fn router` exists in many crates
- **THEN** the gate does not fail it (it is in the denylist).

#### Scenario: Default mode reports, does not fail

- **WHEN** the gate runs in default mode on a tree with cross-crate
  duplicates
- **THEN** it prints warnings and exits 0.

#### Scenario: Tool absent

- **WHEN** `rg` is not installed
- **THEN** the step prints `step: reuse status: skipped` and exits 0.

### Requirement: Intra-Crate Layering Gate

The four-layer DDD architecture is enforced structurally for the core
crates, but new bounded contexts are folders *inside*
`openpanel-domain` / `openpanel-app`, which cargo cannot isolate.
`make check` SHALL run `scripts/check-layering.sh`, which asserts:
- no file under `crates/openpanel-domain/src/**` contains `use
  openpanel_app` or `use openpanel_api`,
- no file under `crates/openpanel-app/src/**` contains `use
  openpanel_api`.

A violation prints `step: layering status: failed` with the offending
file and exits non-zero. The script MUST degrade to `status: skipped`
when `rg` is unavailable.

#### Scenario: Domain imports app code

- **WHEN** a domain module adds `use openpanel_app::some_service`
- **THEN** `make check` fails at the `layering` step naming the file;
  the import must be removed (move the logic or use a trait).

#### Scenario: App imports api code

- **WHEN** an app service imports `openpanel_api`
- **THEN** the `layering` step fails.

### Requirement: Spec-To-Test Drift Gate

`make check` SHALL run `scripts/check-spec-test-drift.sh`, which walks
every archived spec under `openspec/specs/*/spec.md`, counts its
`#### Scenario:` entries, and verifies that at least one test in the
workspace references the capability (by grepping for the capability
name in `tests/` and `#[cfg(test)]` modules). It prints a per-spec
report and, under `--strict` (used for specs created after this
change), fails when a scenario has no covering test. Pre-existing specs
are reported but do not fail the build, to avoid retroactive breakage.

#### Scenario: New spec has no test

- **WHEN** a spec archived after this change declares scenarios but no
  test references its capability
- **THEN** `make check` fails at the `spec-test-drift` step.

#### Scenario: Pre-existing spec gap is reported, not failed

- **WHEN** an older spec has scenarios with no covering test
- **THEN** the script prints a warning line but exits 0.
