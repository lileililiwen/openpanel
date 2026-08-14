# Add Database Point-in-Time Recovery — Design

## BinlogStream model

```rust
pub struct BinlogStream {
    pub db_id: DatabaseId,
    pub engine: DbEngine,            // MySQL | MariaDB | Postgres
    pub target: BackupTargetId,      // from add-offsite-backup-targets
    pub last_flushed: LogSeq,        // GTID / WAL LSN checkpoint
    pub continuous: bool,            // stream vs. periodic flush
    pub status: StreamStatus,        // Active | Paused | Broken
}
```

## PITR restore model

```rust
pub struct PitrRestore {
    pub db_id: DatabaseId,
    pub request_ts: RestoreTimestamp,
    pub base_backup: BackupId,
    pub replay_to: LogSeq,           // resolved from request_ts
    pub staging_db_id: DatabaseId,   // restore lands here first
    pub status: RestoreStatus,
}
```

## Continuous streaming / restore flow

```
stream(db_id):
  enable engine log shipping (binlog ROW / Postgres archive_command)
  tail the log; flush to target every N seconds or per-MB
  checkpoint last_flushed (LogSeq) in BinlogStream row

restore(db_id, request_ts):
  base = latest full backup <= request_ts
  staging = create staging database
  load base into staging
  replay log segments base..request_ts (stop at request_ts, inclusive)
  audit PitrRestoreRequested{request_ts, staging_db_id}
  on owner confirm: promote staging -> live (atomic rename of schema)
  on error: drop staging, audit PitrRestoreFailed{reason}
```

Restores never touch the live database until an explicit owner
confirmation; the swap reuses the same atomic-rename discipline as
`site-staging`.

## Endpoints

```
POST /api/v1/backups/databases/{id}/pitr/restore
     body { timestamp, confirm? }
GET  /api/v1/backups/databases/{id}/binlog
```

## Tests

```
1.1 Unit: LogSeq ordering; timestamp -> replay-window resolution;
    staging-before-swap invariant.
1.2 Property: streamed logs stay within the chosen target; replay is
    monotonic and idempotent for a fixed timestamp.
1.3 Service tests w/ mock engine + mock target: stream, restore to
    staging, promote.
1.4 Integration: live binlog reaches target; restore reproduces a
    row inserted before request_ts but not after.
1.5 CLI E2E: openpanel db pitr inspect -> restore (confirm) -> verify.
1.6 Web: Restore dialog (timestamp picker, staging preview, confirm).
```
