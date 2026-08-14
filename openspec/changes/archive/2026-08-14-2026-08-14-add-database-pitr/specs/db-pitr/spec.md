## ADDED Requirements

### Requirement: Continuous Binlog Streaming

The system SHALL continuously stream the database engine's transaction
log (MySQL/MariaDB binlog, PostgreSQL WAL) for a given database to a
configured backup target, checkpointing the last flushed `LogSeq`.
Streaming SHALL pause cleanly and resume without gaps; the destination
SHALL be one of the offsite targets defined by
`add-offsite-backup-targets`.

#### Scenario: Streaming starts

- **WHEN** an Owner enables streaming for database `db1` to target `t1`
- **THEN** a `BinlogStream` row exists with `status=Active`, and
        transaction logs begin arriving at `t1`; audit
        `BinlogStreamEnabled{db_id, target}`.

#### Scenario: Stream interrupted and resumed

- **WHEN** the stream connection to the engine drops
- **THEN** the stream is marked `Broken`, and on reconnect it resumes
        from the last checkpointed `LogSeq` without skipping segments.

### Requirement: Point-in-Time Restore

`POST /backups/databases/{id}/pitr/restore` SHALL restore the database
to a caller-chosen `timestamp` by loading the latest full backup at or
before the timestamp and replaying transaction logs up to that second.
The restore SHALL land in a staging database first and SHALL NOT
overwrite the live database until an explicit owner confirmation
promotes it.

#### Scenario: Restore to a timestamp

- **WHEN** an Owner posts a restore with `timestamp=T` for `db1`
- **THEN** a `PitrRestore` is created with a `staging_db_id`, the
        replay window is resolved from the base backup to `T`, and a
        staging database reflects state at `T`; audit
        `PitrRestoreRequested{db_id, request_ts}`.

#### Scenario: Promotion overwrites live

- **WHEN** the Owner confirms the staging restore
- **THEN** the staging database is atomically promoted to live and the
        previous live database is retained for rollback; audit
        `PitrRestorePromoted{restore_id}`.

### Requirement: Incremental File Backup

The system SHALL offer an incremental file-backup mode that captures
block-level deltas of the database data directory alongside the existing
full dump, so backups between full snapshots are small and fast.

#### Scenario: Incremental capture

- **WHEN** a scheduled backup runs with `mode=incremental` for `db1`
- **THEN** only deltas since the last backup are stored at the target,
        and an `IncrementalBackup` row records the base and delta refs.

### Requirement: Binlog Inspection

`GET /backups/databases/{id}/binlog` SHALL return the available
transaction-log range (earliest and latest `LogSeq`, plus the covered
time span) so a caller can choose a valid restore timestamp.

#### Scenario: Inspect range

- **WHEN** an Owner requests the binlog range for `db1`
- **THEN** the response lists the earliest/latest `LogSeq` and the
        covered time window, and any timestamp outside that window is
        reported as unrestorable.
