## ADDED Requirements

### Requirement: Governance Concerns Have Executable Protection

Every manifest-listed archived governance requirement SHALL map to an
executable positive/negative checker in the repository’s gate self-test.
Passing a text or archive merge check alone SHALL NOT be considered evidence
that the governance concern is protected from later code or configuration
regression.

#### Scenario: Text-only positive assessment is insufficient

- **WHEN** an archived governance requirement remains present in the live spec
  but its mapped checker is removed or no longer exercises a negative case
- **THEN** the mandatory governance gate fails.

#### Scenario: Positive and negative protection exists

- **WHEN** the mapped checker proves both compliant and violating fixtures
- **THEN** the requirement is reported as executable-protected.
