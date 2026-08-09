## ADDED Requirements

### Requirement: Allowlisted Service Inventory

The system SHALL list only registered service descriptors with display name, fixed unit identifier, load/active/sub states, enablement, supported actions, last transition, health, and dependent capabilities. Client input MUST NOT select a raw unit name.

#### Scenario: Unknown service ID

- **WHEN** a caller requests an unregistered descriptor ID
- **THEN** no system command runs and the service is reported not found

### Requirement: Controlled Lifecycle Actions

Authorized callers SHALL preview and perform supported start, stop, restart, reload, enable, and disable actions using fixed argv, timeout, post-action refresh, readiness probe, and audit. Disruptive actions SHALL require confirmation and show affected resources.

#### Scenario: Restart nginx

- **WHEN** an Owner confirms restart after a preview listing hosted sites
- **THEN** nginx is restarted, readiness is checked, final state is returned, and the action is audited

#### Scenario: Action times out

- **WHEN** the controller exceeds its deadline
- **THEN** the process is terminated, actual state is refreshed, and the result is recorded as indeterminate or failed

### Requirement: Service Health Monitoring

The system SHALL periodically check registered services, apply hysteresis to transitions, retain bounded health history, emit failure/recovery alerts, and optionally auto-restart only services explicitly configured with attempt budgets and cooldown.

#### Scenario: Persistent failure

- **WHEN** a service fails the configured consecutive checks
- **THEN** one failure transition and alert are recorded rather than one per poll

### Requirement: Service Surfaces

REST, CLI, and `/services` web surfaces SHALL expose inventory, detail, impact preview, authorized actions, health history, and bounded redacted journal entries. Owner-only actions SHALL be hidden from other roles and enforced server-side.

#### Scenario: Admin views service

- **WHEN** an Admin views a registered service
- **THEN** status, health, dependencies, and permitted actions are returned without unrestricted unit controls

#### Scenario: Journal entry contains a credential

- **WHEN** a bounded journal result matches a configured secret pattern
- **THEN** the secret is redacted before API, CLI, web, or audit output
