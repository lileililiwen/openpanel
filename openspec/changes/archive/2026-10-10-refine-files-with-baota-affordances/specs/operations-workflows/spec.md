# operations-workflows Specification

## ADDED Requirements

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
