# operations-workflows Specification

## Purpose
TBD - created by archiving change complete-file-database-backup-workflows. Update Purpose after archive.
## Requirements
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

### Requirement: Application Delivery Workflow Is Confirmed End-to-End

The application delivery workflow MUST require a typed
operator confirmation at every mutating step (build, promote,
rollback). The confirmation is captured in the request, recorded
in the audit event, and never exposed in the response.

#### Scenario: Promote without confirmation is refused

- **WHEN** an operator submits a promote request with an empty
  `confirmation` field
- **THEN** the request is refused with a typed
  `ApplicationDeliveryError` and no release state mutation
  occurs.

#### Scenario: Rollback without confirmation is refused

- **WHEN** an operator submits a rollback request with an empty
  `confirmation` field
- **THEN** the request is refused with a typed
  `ApplicationDeliveryError` and no release state mutation
  occurs.

