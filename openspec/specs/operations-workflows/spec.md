# operations-workflows Specification

## Purpose

aaPanel and BaoTa reduce operational work with recycle bin, archive operations, remote download, content search, database import/export/restore, permissions, backup destinations, capacity checks, progress, cancellation, and restore verification. OpenPanel has primitives for files, databases, backups, PITR, and drills, but the UI does not present a coherent end-to-end workflow.
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

### Requirement: Parity File Actions Carry Previews

Clipboard (copy/cut/paste), compress, extract, recycle,
restore, permission-apply, history-restore, and remark
actions MUST validate through `validate_file_action`
previews: scope count, recoverability, and confirmation
requirement visible before execution. Empty selections
MUST be rejected with a safe reason.

#### Scenario: Empty clipboard paste is rejected

- **WHEN** a paste is requested with an empty clipboard
- **THEN** the preview rejects with `No files selected`
  and nothing executes.

### Requirement: Archive And Extract Report Progress

Compress and extract MUST render `TaskState` lifecycle
banners (queued, running percent, success, secret-free
failure) so long operations stay observable.

#### Scenario: Running archive shows percent

- **WHEN** a compress runs at 42 percent
- **THEN** the banner renders the percent state with
  `role="status"`.

### Requirement: Recycle Workflow Is Confirm Then Restore

Delete MUST preview as destructive and recoverable,
require confirmation, land entries in the recycle bin,
and offer restore plus confirmed empty. Unconfirmed
delete MUST mutate nothing.

#### Scenario: Unconfirmed delete is refused

- **WHEN** a delete is submitted without confirmation
- **THEN** the workflow refuses with the confirmation
  reason and the entry stays in place.

### Requirement: Capacity Header Gates Archive Work

The file-manager header MUST evaluate selection size via
`evaluate_backup_capacity` and block archive work with
the actionable capacity message when estimate exceeds free
space, instead of failing mid-flight.

#### Scenario: Insufficient space blocks archive

- **WHEN** the estimated archive exceeds free space
- **THEN** execution is blocked with the capacity reason
  naming need versus have, and no archive is created.

