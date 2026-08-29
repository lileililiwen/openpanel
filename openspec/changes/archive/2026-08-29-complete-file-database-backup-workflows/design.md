# Design: File, database, and backup workflows

## Explore & Reuse

- Reuse `FilesService`, path validation, existing file list/read/write/upload handlers, and confirmation layer.
- Reuse `DatabasesService`, encrypted password/reveal contract, database privilege and PITR services.
- Reuse `BackupService`, restore drills, offsite target services, job status, loading indicators, and progress markup.
- Reuse existing table/card/form tokens and UI-state components.

## Workflow contracts

Files: scope breadcrumb → selection → action preview → progress/result. Database: metadata-only list → action wizard → one-time secret behavior → task result. Backup: select scope → destination → retention → estimated size/free-space check → preview → execute → progress → verify/restore.

All actions are idempotent where the service supports it, show correlation IDs on failure, and retain the user's context. Capacity and destination failures are actionable and never represented as generic “something went wrong”.

## Security

Use existing CSRF and confirmation helpers. Keep passwords and keys out of list/detail HTML, logs, audit metadata, and error text. Cross-owner paths and database IDs are denied uniformly.

## Verification

Integration tests exercise happy paths, partial failures, cancellation, retries, empty data, capacity failure, cross-owner denial, and secret non-leakage.
