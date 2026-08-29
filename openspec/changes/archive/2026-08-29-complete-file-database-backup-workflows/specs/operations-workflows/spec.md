## ADDED Requirements

### Requirement: File actions are recoverable

File management MUST support scoped selection, clear action previews, progress/result states, and confirmation for destructive actions.

#### Scenario: User deletes selected entries

- **WHEN** an authorized user confirms deletion
- **THEN** the action is scoped, visible in progress/result feedback, and recoverable where supported

### Requirement: Database actions protect secrets

Database import, export, backup, restore, and password operations MUST preserve existing encryption and one-time secret rules.

#### Scenario: Database list renders

- **WHEN** a database list is requested
- **THEN** plaintext passwords and private credentials are absent

### Requirement: Backup execution is capacity-aware

Backup creation MUST show scope, destination, retention, estimated size, available space, progress, cancellation, and verification outcome.

#### Scenario: Insufficient disk space

- **WHEN** the estimated backup exceeds available capacity
- **THEN** execution is blocked with an actionable capacity message
