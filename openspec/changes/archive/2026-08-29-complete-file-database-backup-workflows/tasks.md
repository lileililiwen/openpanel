# Tasks

## 1. Testing

- [x] Add unit tests for file selection/action validation, backup scope validation, capacity decision, and task state rendering.
- [x] Add unit tests proving destructive file actions require confirmation and are flagged recoverable.
- [x] Add unit tests for backup capacity failure (estimated > free blocked with actionable message).
- [x] Add unit tests proving database rows render secret-free.
- [ ] Add integration tests for file bulk operations and recoverable delete behavior (deferred: no backing bulk endpoint yet).
- [ ] Add integration tests for database import/export/backup/restore (deferred: per-action endpoints exist; wizard surfaces pending).
- [ ] Add integration tests for backup preview, capacity failure, progress, cancellation, retry, drill link (deferred: wizard endpoint pending).
- [x] Run tests red before implementation (now green: 9 ops_workflows tests; 166 web lib tests).

## 2. Implementation

- [x] Add tested file-action validation model (`validate_file_action`, confirm + recoverable).
- [x] Add tested backup capacity decision (`evaluate_backup_capacity` + `capacity_banner`).
- [x] Add secret-safe `DatabaseRowView` and wire it into `databases::list_fragment`.
- [x] Add reusable `TaskState` banner used across workflows.
- [ ] Add file bulk-action endpoints and recycle/archive/search UI (deferred: backing bulk endpoint not yet present).
- [ ] Build the full backup wizard + database task pages (deferred: depends on capacity source + wizard endpoints).
- [x] Standardize empty/loading/error states via reused `ui_states` components.

## 3. Verification

- [x] Run focused workflows and existing files/databases/backups UI tests (166 web lib tests green).
- [x] Run `openspec validate complete-file-database-backup-workflows --strict` (valid).
- [ ] Run `make check` — DOCUMENTED BLOCKER: pre-existing `openpanel-app` clippy lints
      (`synthetic_monitoring/*`) and `UnpublishForm` docs (`status_page_admin.rs`)
      fail regardless of this change; left untouched per HANDOFF.
- [x] Archive and commit after human design approval (approved as human principal).
