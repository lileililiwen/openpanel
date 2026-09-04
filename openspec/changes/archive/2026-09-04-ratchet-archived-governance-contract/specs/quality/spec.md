## ADDED Requirements

### Requirement: Archived Governance Content Ratchet

The repository SHALL maintain a reviewed manifest of every archived
governance requirement in the `agent-quality`, `quality`, `testing`, and
`architecture` capabilities. Each entry MUST identify the archive path,
capability, exact requirement name, content digest, scenario count, and at
least one executable checker ID. `make check` SHALL run a read-only gate that
fails when a live requirement block is missing, its normalized content or
scenario count differs from the manifest, an archive path is unknown, or a
checker ID is not implemented by the gate self-test.

#### Scenario: Requirement text is weakened

- **WHEN** a later edit removes a MUST/SHALL obligation from a manifest-listed
  governance requirement without a reviewed manifest update
- **THEN** the governance contract gate fails and names the requirement.

#### Scenario: Scenario protection is removed

- **WHEN** a scenario is deleted from a manifest-listed governance requirement
- **THEN** the gate fails on the scenario-count/content mismatch.

#### Scenario: Executable checker is orphaned

- **WHEN** a manifest entry names a checker ID absent from the gate self-test
- **THEN** the gate fails and names the orphaned checker ID.

#### Scenario: Reviewed update passes

- **WHEN** a reviewed OpenSpec change updates the manifest digest and checker
  mapping together with the live requirement
- **THEN** the gate passes without modifying the manifest or source files.
