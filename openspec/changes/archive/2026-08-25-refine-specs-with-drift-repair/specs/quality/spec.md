## ADDED Requirements

### Requirement: Archive-Time Delta Merge Gate

The quality pipeline SHALL verify that every requirement introduced by
an archived OpenSpec change's deltas exists in the corresponding live
spec under `openspec/specs/<cap>/spec.md`. Pre-existing gaps SHALL be
recorded in a committed baseline file and reported without failing;
any drift not present in the baseline SHALL fail `make check` naming
the capability, requirement, and originating archive path. The
baseline SHALL only shrink over time.

#### Scenario: New un-merged delta fails the build

- **WHEN** an archived change's delta contains a requirement absent
        from both the live spec and the baseline
- **THEN** `check-spec-drift` exits non-zero and names both files.

#### Scenario: Baseline gap reported without failing

- **WHEN** a missing requirement is listed in the committed baseline
- **THEN** the gate reports it as tracked debt and exits 0.

#### Scenario: Clean tree passes

- **WHEN** every archived delta's requirements are present in their
        live specs
- **THEN** the gate exits 0 without modifying any file.
