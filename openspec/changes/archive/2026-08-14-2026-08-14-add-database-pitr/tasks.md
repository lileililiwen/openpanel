# Add Database Point-in-Time Recovery — Tasks

## 1. Testing

- [x] 1.1 Unit: `LogSeq` ordering and clamping; timestamp →
      replay-window resolution; assert restore writes to staging before
      any swap.
- [x] 1.2 Property: streamed log segments never escape the chosen
      backup target; replaying the same timestamp twice is idempotent
      and yields an identical schema state.
- [x] 1.3 Service: enable stream, flush to target, restore to a
      staging database, promote on confirm; audit events recorded.
- [x] 1.4 Integration: a binlog insert before `request_ts` is present
      after restore and an insert after `request_ts` is absent.
- [x] 1.5 CLI E2E: `openpanel db pitr inspect` → `restore --confirm`
      → verify row state.
- [x] 1.6 Web: Restore dialog (timestamp picker, staging preview,
      explicit confirm), binlog range viewer.

## 2. Domain and Application

- [x] 2.1 Implement `BinlogStream`, `PitrRestore`, `IncrementalBackup`,
      `RestoreTimestamp`, `BinlogRange` under
      `crates/openpanel-domain/src/db_pitr/`.
- [x] 2.2 Add SQLite migration for `binlog_streams`, `pitr_restores`,
      `incremental_backups`.
- [x] 2.3 Implement `PitrService`, `BinlogStreamer`,
      `IncrementalFileLayer`; register via `ModuleRegistry`.

## 3. Adapters and UI

- [x] 3.1 Add `/backups/databases/{id}/pitr/restore` and
      `/backups/databases/{id}/binlog` REST routes.
- [x] 3.2 Add `openpanel db pitr {status,restore,inspect}`.
- [x] 3.3 Build the Restore dialog (timestamp picker, staging preview,
      confirm), binlog range view.

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean.
- [x] 4.3 Smoke-test: enable streaming on a test DB, insert a row,
      restore to a timestamp, confirm the row is present and a
      post-timestamp insert is absent.
- [x] 4.4 Archive with `openspec archive add-database-pitr`.
