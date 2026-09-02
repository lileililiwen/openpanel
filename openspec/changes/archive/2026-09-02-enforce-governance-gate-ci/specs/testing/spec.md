## ADDED Requirements

### Requirement: Governance Gate Tests Cover Positive And Negative Paths

Every shell governance gate introduced by the project SHALL have a
fixture-based self-test covering at least one compliant input and one
non-compliant input. The self-test SHALL assert exit status and SHALL use
isolated temporary fixtures so repository state cannot mask a regression.

#### Scenario: Compliant fixture passes

- **WHEN** a fixture satisfies a gate’s documented contract
- **THEN** the self-test asserts exit code zero.

#### Scenario: Non-compliant fixture fails

- **WHEN** a fixture violates a gate’s documented contract
- **THEN** the self-test asserts a non-zero exit code and a useful failure
  diagnostic.
