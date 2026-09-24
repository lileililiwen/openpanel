# quality-maturity-ratchet Specification

## Purpose
TBD - created by archiving change ratchet-quality-and-spec-maturity. Update Purpose after archive.
## Requirements
### Requirement: New and Modified Spec Scenarios Are Covered

The quality chain MUST fail when a newly added or modified capability has a
scenario with no executable covering test.

#### Scenario: New uncovered scenario

- **WHEN** a change adds a capability scenario without a test reference
- **THEN** the strict drift gate fails and names the capability and scenario.

### Requirement: Quality Debt Only Shrinks

Tracked baselines for uncovered scenarios and accepted duplicate items MUST
be versioned, and a change MUST NOT increase them.

#### Scenario: Debt increases

- **WHEN** a patch adds a new baseline entry without removing an existing one
- **THEN** the ratchet fails with the newly introduced debt.

### Requirement: Coverage Is Reproducible and Enforced

Required CI coverage MUST use a pinned supported tool and enforce configured
workspace and crate floors; an unavailable tool MUST fail the required job.

#### Scenario: Coverage tool unavailable

- **WHEN** the required coverage job cannot invoke its configured tool
- **THEN** CI fails instead of uploading a stub report.

### Requirement: Duplicate Logic Is Classified

Mandatory quality checks MUST fail on unclassified duplicate public items and
MUST allow only documented generated, fixture, or compatibility exceptions.

#### Scenario: New duplicate public function

- **WHEN** a new public function duplicates an existing implementation
- **THEN** the reuse gate fails and names both owners.

### Requirement: Incomplete Work Has Evidence

Stubs, TODO markers, test fakes, and environment-blocked checks MUST be
classified in an evidence document with owner, reason, scope, and closure
condition.

#### Scenario: Untracked production stub

- **WHEN** a production stub is introduced without an evidence record
- **THEN** the maturity gate fails.

### Requirement: Ratchet Gates Emit Release Evidence

The ratchet-protected gates (coverage-floor, maturity, layering, reuse,
spec-test-drift) whose CI results are required publication evidence
MUST produce records consumable by the `release-evidence` contract. A
gating regression in any of these ratchets MUST surface as a
non-`PASS` `coverage` or `maturity` record in
`dist/evidence-manifest.json` and MUST block publication. The detailed
schema and the stale-evidence rejection rules are owned by
`openspec/specs/release-evidence/spec.md`.

#### Scenario: Coverage floor regression blocks upload

- **WHEN** `make coverage-floor` exits non-zero for the release commit
- **THEN** the `coverage` evidence record is recorded as `FAIL` (or
  `BLOCKED` when the tool is missing) and publication is blocked.

#### Scenario: Maturity regression blocks upload

- **WHEN** a new untracked TODO or stub lands in production
- **THEN** the `maturity` evidence record is recorded as `FAIL` and
  publication is blocked.

