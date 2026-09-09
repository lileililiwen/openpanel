## ADDED Requirements

### Requirement: New Maturity Gates Have Executable Protection

The new governance gates introduced by
`ratchet-quality-and-spec-maturity` (`spec-test-drift-strict`,
`reuse-strict`, `coverage-floor`, `maturity`) MUST each register a
positive and a negative fixture in `scripts/test-gates.sh`, tagged
with `# checker: <gate> positive` / `# checker: <gate> negative`
markers so `make governance-contract` can verify that every
manifest-listed requirement still maps to a checker that exercises
both the compliant and the violating case. A new gate without a
paired fixture pair fails the build.

#### Scenario: Paired fixtures are required

- **WHEN** a new governance gate is added to the mandatory `make
  check` chain
- **THEN** the change also adds both a positive and a negative
  fixture to `scripts/test-gates.sh` with the `# checker:` markers.

#### Scenario: Orphaned checker is rejected

- **WHEN** a manifest entry names a checker id absent from the
  fixture registry (or whose only fixture is the positive one)
- **THEN** `make governance-contract` fails and names the orphaned
  id, exactly as for every other governance gate.
