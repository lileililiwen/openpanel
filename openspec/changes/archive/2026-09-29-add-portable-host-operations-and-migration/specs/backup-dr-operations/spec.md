# backup-dr-operations Specification

## Requirements

## ADDED Requirements

### Requirement: Host Migration Plans Reuse Backup Verification

A host migration plan that moves sites, databases, mail, DNS, or SSL between
hosts MUST reuse the existing backup-health and collision-preview surfaces
rather than inventing a parallel planner. Post-import verification MUST gate any
subsequent DNS, mail, or SSL cutover, and a plan that fails verification MUST
remain isolated with the source bundle intact.

#### Scenario: Migration plan evidence reuses backup health

- **WHEN** an operator previews a host migration plan
- **THEN** the plan's resources and conflicts reuse the existing backup health
  and collision-preview evidence, and a failed post-import verification blocks
  cutover while the source bundle remains recoverable.
