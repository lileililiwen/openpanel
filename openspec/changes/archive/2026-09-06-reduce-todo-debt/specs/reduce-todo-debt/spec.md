## ADDED Requirements
### Requirement: TODO tracking
Source TODO/FIXME markers SHALL be tracked and resolved or converted to issues.
#### Scenario: TODO found
- **WHEN** a TODO/FIXME marker exists
- **THEN** it is either fixed or linked to a tracked issue
#### Scenario: New TODO added
- **WHEN** code is committed with a TODO
- **THEN** CI lint flags it for review
