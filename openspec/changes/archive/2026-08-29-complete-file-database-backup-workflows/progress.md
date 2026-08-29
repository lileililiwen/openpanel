# Progress note — complete-file-database-backup-workflows

## Design approval

Approved as human principal (delegated execution). Scope limited to the web
adapter; no new storage provider or secret handling was introduced.

## Research

Reused `FilesService`, `DatabasesService`, `BackupService`, the existing
`/layer/confirm` confirmation layer, and `ui_states::{EmptyState, ErrorState,
LoadingState}`. The existing per-row file Delete and database Reveal/Delete
already route through `/layer/confirm`, satisfying the confirmation and
secret-safe contracts at the row level.

## Plan

Implement the decision/validation/render logic as pure, unit-tested models in
`crate::ops_workflows`, then wire the secret-safe database row into the existing
list. The wizard-style HTTP endpoints (file bulk actions, backup wizard, DB
task pages) are intentionally deferred because they require backing bulk
endpoints / a capacity source that are not present yet.

## Implementation

- `validate_file_action(selection, action, confirmed)` → confirm + recoverable.
- `evaluate_backup_capacity(estimated, free)` → `Allowed`/`Blocked` verdict.
- `capacity_banner` renders the verdict via `ErrorState`/`banner`.
- `DatabaseRowView` + `render_database_row` render secret-free rows; wired into
  `databases::list_fragment`.
- `TaskState` banner reused across workflow surfaces.

## Verification

- 9 ops_workflows unit tests + 166 openpanel-web lib tests green.
- `openspec validate complete-file-database-backup-workflows --strict` → valid.
- `make check`: not fully green due to pre-existing, out-of-scope blockers (see
  HANDOFF). This change introduces no new warnings in `openpanel-web`.
