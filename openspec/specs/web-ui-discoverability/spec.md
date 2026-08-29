# web-ui-discoverability Specification

## Purpose
TBD - created by archiving change repair-ui-discoverability. Update Purpose after archive.
## Requirements
### Requirement: Shipped capabilities are discoverable

The web shell MUST register every shipped first-class page capability and expose it through role-filtered navigation.

#### Scenario: Owner sees shipped workflows

- **WHEN** an owner loads the shell
- **THEN** every mounted first-class owner workflow has a reachable navigation item

### Requirement: Navigation metadata is valid

Every navigation item MUST have a unique route, a valid capability key, and a built-in icon.

#### Scenario: Invalid item is rejected

- **WHEN** navigation metadata is tested
- **THEN** duplicate routes, unknown capabilities, and missing icons fail the test suite

### Requirement: Role filtering remains enforced

Owner-only items MUST be absent for user-role principals while direct routes remain authorization-protected.

#### Scenario: User loads shell

- **WHEN** a user-role principal loads the shell
- **THEN** owner-only navigation is absent and user-scoped navigation remains visible

