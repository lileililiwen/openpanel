## ADDED Requirements

### Requirement: Governance Gate Self-Tests Are Mandatory

The repository SHALL expose `make test-gates`, running
`scripts/test-gates.sh`, and `make check` SHALL run it. The self-test MUST
exercise both passing and failing fixtures for every governance gate it
claims to cover. A broken or missing gate target MUST cause a non-zero result;
the self-test MUST NOT silently skip because a target is absent.

#### Scenario: Orphaned self-test is rejected

- **WHEN** `scripts/test-gates.sh` exists but `make test-gates` is undefined
- **THEN** the quality configuration is invalid and the mandatory check fails.

#### Scenario: Gate behavior regresses

- **WHEN** a fixture that violates a governance rule causes its gate to exit
  zero
- **THEN** `make test-gates` exits non-zero and identifies the failed fixture.

### Requirement: CI Enforces The Single Quality Entry Point

The required CI check for every push and pull request SHALL run `make check`
and SHALL fail on OpenSpec validation or governance gate failures. Required
validation steps MUST NOT use `continue-on-error`; only explicitly
informational coverage MAY remain non-blocking. Strict options SHALL be used
for checks whose archived specification says new changes are strict.

#### Scenario: OpenSpec validation fails

- **WHEN** an active change is malformed
- **THEN** CI is red and the pull request cannot satisfy the quality check.

#### Scenario: Required gate fails

- **WHEN** `make check` returns non-zero for a governance regression
- **THEN** the required CI job fails rather than allowing another job to pass.
