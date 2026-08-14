# Add Database Point-in-Time Recovery

## Why

`refine-backups-with-resource-restore-and-policies` (active) adds
offsite targets and resource-scoped restore; `add-offsite-backup-targets`
(active) adds S3/B2/rsync destinations — but there is **no point-in-time
or incremental database backup**. Today a database restore can only
return the cluster to the last full snapshot, losing everything between
snapshots. For production databases this is unacceptable: a dropped table
or a bad migration at 14:02 cannot be undone without losing the 13:30–
14:02 window. This change adds a `db-pitr` bounded context.

## What Changes

- New bounded context `db-pitr` carrying the `BinlogStream`,
  `PitrRestore`, and `IncrementalBackup` aggregates.
- Continuous binlog (MySQL/MariaDB) streaming to a configured backup
  target; PostgreSQL WAL archiving where the engine supports it.
- Point-in-time restore to a caller-chosen timestamp, built from the
  last full backup plus replayed binlog/WAL up to the target second.
- An optional incremental file-backup mode (block-level deltas) for the
  data directory, alongside the existing full dumps.
- New endpoints: `POST /backups/databases/{id}/pitr/restore`
  (body: `timestamp`), `GET /backups/databases/{id}/binlog`.

## Capabilities

### New Capabilities

- `db-pitr`: continuously stream engine transaction logs to a backup
  target, restore a database to an arbitrary timestamp, and take
  incremental deltas; inspect the available binlog/WAL range.

## Impact

- Domain: `BinlogStream`, `PitrRestore`, `IncrementalBackup`,
  `RestoreTimestamp`, `BinlogRange`.
- App: `PitrService`, `BinlogStreamer`, `IncrementalFileLayer`.
- API/CLI/web: `/backups/databases/{id}/pitr/restore`,
  `/backups/databases/{id}/binlog`; CLI
  `openpanel db pitr {status,restore,inspect}`; web Restore dialog.
- Security: binlog/WAL streams inherit the encryption-at-rest and
  offsite auth of the destination target; a restore is audited and
  never overwrites the live database in place (restores into a staging
  DB, then swaps).
- Coupling: depends on the `backups` cap; reuses offsite destinations
  from `refine-backups-with-resource-restore-and-policies` and
  `add-offsite-backup-targets`; coordinates with the `databases` cap for
  engine credentials and connection control.
