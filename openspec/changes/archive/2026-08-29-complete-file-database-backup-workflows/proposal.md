# Complete file, database, and backup workflows

## Why

aaPanel and BaoTa reduce operational work with recycle bin, archive operations, remote download, content search, database import/export/restore, permissions, backup destinations, capacity checks, progress, cancellation, and restore verification. OpenPanel has primitives for files, databases, backups, PITR, and drills, but the UI does not present a coherent end-to-end workflow.

## What

Complete the three highest-frequency operational workflows with bulk actions, clear scope, progress, capacity warnings, and recoverable outcomes.

## Capabilities

### New

- File bulk actions, archive/recycle-bin/search affordances where backend support exists.
- Database import/export/backup/restore task surfaces.
- Backup wizard with destination, retention, capacity preview, progress, cancel, and restore verification.

### Modified

- Existing tables become responsive task surfaces with consistent empty/loading/error states.

## Non-goals

- No new storage provider implementation.
- No silent destructive deletion.
- No plaintext secret persistence or rendering.

## Dependencies

Depends on `repair-ui-discoverability` and may consume `add-audit-activity-center` links.
