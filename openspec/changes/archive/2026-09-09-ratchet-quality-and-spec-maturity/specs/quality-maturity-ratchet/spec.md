# quality-maturity-ratchet Specification

## Requirements

## ADDED Requirements

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
