# Proposal: Complete backup DR and migration operations

## Why

OpenPanel has backup plans, offsite targets, snapshots, restore previews, and
restore-drill domain logic, but operators need a reliable end-to-end workflow:
verify remote storage, see RPO/RTO, schedule drills, inspect failures, and
migrate a host. Mature panels make granular restore and remote backup behavior
visible and actionable.

## What Changes

- Add remote target connectivity and retention verification.
- Add backup health, RPO/RTO, last-success, and next-run summaries.
- Complete scheduled/on-demand restore drill surfaces and failure alerts.
- Add scoped restore progress, collision preview, and audit details.
- Complete host-to-host migration export/import/bootstrap workflow.

## Capabilities

### Modified Capabilities

- `backups`
- `offsite-backup-targets`
- `server-snapshots`
- `migration-importers`
- `notifications`

## Non-goals

- New backup archive format.
- Silent destructive restore.
- Provider-specific proprietary storage protocol.

## Dependencies

Depends on the quality ratchet and release/deployment governance. It reuses
existing backup and migration contracts.
