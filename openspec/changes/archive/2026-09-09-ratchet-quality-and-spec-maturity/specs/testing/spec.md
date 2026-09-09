## ADDED Requirements

### Requirement: Production Incomplete Work Has Evidence

Every production `// TODO(openpanel#<id>[: ...])`, `// FIXME(...)`,
or `// stub(...)` marker in `crates/` MUST have a matching reviewed
entry in `openspec/governance/evidence.yaml`. The marker regex
captures the bare id (without the `openpanel#` prefix) and the
gate (`scripts/check-maturity.sh`) matches the record by id; the
record's `location` field is informational so a marker move does
not require a manifest update.

A new marker introduced without an evidence record MUST fail
`make maturity`. A stale record (whose marker no longer exists) is
advisory, not fatal: the gate reports it so the next reviewer can
remove it, but does not fail the build.

#### Scenario: Tracked marker passes

- **WHEN** a production marker `// TODO(openpanel#ACME-HTTP01)` is
  present and the evidence manifest has a record with
  `id: ACME-HTTP01`
- **THEN** `make maturity` exits zero.

#### Scenario: Untracked marker fails

- **WHEN** a production marker `// TODO(openpanel#NEW-ID)` is
  introduced and the evidence manifest has no record with
  `id: NEW-ID`
- **THEN** `make maturity` fails and names the new id and at least
  one source location.

#### Scenario: Stale record is advisory

- **WHEN** an evidence record's marker has been removed (e.g. the
  follow-up change landed) and no new marker carries the same id
- **THEN** `make maturity` reports the stale record but exits zero;
  a reviewer removes the entry in a follow-up change.

#### Scenario: Evidence manifest is malformed

- **WHEN** `openspec/governance/evidence.yaml` is not a valid YAML
  mapping with `version: 1` and a `records` list
- **THEN** `make maturity` fails with a diagnostic that names the
  manifest path and the parse error.
