# Add Server snapshot & migration — Progress

## Status: ~85% Complete

## What's Done

### Domain (crates/openpanel-domain/src/backups/snapshot.rs)
- `SnapshotManifest` with schema validation
- `SnapshotEntry` with sha256 verification
- `ConfirmToken` with single-use consumption
- `PreflightReport` for restore readiness
- `preflight()` pure function for host/version/collision checks
- 14 unit tests + 100-case property test (all passing)

### Application (crates/openpanel-app/src/backups/server_snapshot.rs)
- `ServerSnapshotService` with create/list/get/preflight/restore
- `backups()` accessor for CLI integration
- SHA-256 hash verification on restore
- Single-use confirmation token enforcement
- Audit events for snapshot operations

### REST API (crates/openpanel-api/src/routes/server_snapshots.rs)
- `POST /api/v1/server/snapshots` — create from backup run
- `GET /api/v1/server/snapshots` — list all
- `GET /api/v1/server/snapshots/:id` — get manifest
- `POST /api/v1/server/snapshots/:id/preflight` — preflight check
- `POST /api/v1/server/snapshots/:id/restore` — restore

### CLI (crates/openpanel-cli/src/commands.rs + handlers.rs)
- `openpanel server-snapshot list`
- `openpanel server-snapshot get --id <id>`
- `openpanel server-snapshot create`
- `openpanel server-snapshot preflight --id <id>`
- `openpanel server-snapshot restore --id <id> --confirm <token>`

### Tests
- 2 integration tests passing:
  - `snapshot_routes_require_authentication` (401 on unauthenticated)
  - `snapshot_create_preflight_and_single_use_restore` (full lifecycle)

## What's Left

1. **Migration round-trip test** — two hosts sharing an offsite target
2. **CLI E2E test** — `cli_server_snapshot_create_then_list`
3. **Tamper detection test** — corrupted entry aborts restore
4. **Web UI** — Snapshots management page
5. **`make check`** — full quality gate pass
6. **Archive** — `openspec archive add-server-snapshot-migration`

**Update (session 2026-08-26):** task 1.4 done —
`prop_manifest_bundle_hash_round_trip` (100 cases, canonical bytes +
hash match). Task 1.6 done — restore verifies entries in manifest
order and aborts with `ServerSnapshotError::Partial { applied,
failed }`; integration test tampers the last entry end to end.
Task 1.8 done — CLI E2E create/list/get + preflight/restore confirm
gating. Fixed: `snapshot_caller()` hashed "admin" (below the 12-char
minimum) so every snapshot subcommand panicked.

**Update (session 2026-08-26, later):** tasks 3.3 and 1.7 done —
`SnapshotImporterDriver` implements the `MigrationDriver` contract
(sniff/dry-run/run/rollback with per-entry sha256 re-verification at
commit time) over `SnapshotBundleSource`; offsite glue
`export_to_target`/`import_from_target` rides the existing
`BackupTargetAdapter` as one gzipped-tar object.
`migrate_round_trip_between_hosts_via_shared_offsite_target` proves
export → pull → preview → commit → rollback across hosts.

**Update (session 2026-08-26, final):** task 3.4 done —
`ServerSnapshotService::prune_retention` removes the oldest bundles
beyond a keep-count (auditing `SnapshotPruned` per removal) and
`schedule()` registers a recurring cron command job that runs
`server-snapshot create --retain N`, so every scheduled occurrence
also prunes; CLI gained `create --retain N` and a `schedule`
subcommand.

**Remaining (tasks unchecked):** 1.9/4.3 web Snapshots page (needs UI
work + screenshots), 5.2 full `make check`, 5.3 restore smoke-test in
a container, 5.4 archive.

## Key Decisions

- Snapshots stored under `OPENPANEL__SNAPSHOTS__ROOT` (default: `/var/lib/openpanel/snapshots`)
- Host ID persisted to `<snapshots_root>/host-id` for cross-run stability
- Restore delegates to existing `BackupService::restore` pipeline
- CLI commands use `snapshot_caller()` helper (Owner role required)
