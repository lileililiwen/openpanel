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

## Key Decisions

- Snapshots stored under `OPENPANEL__SNAPSHOTS__ROOT` (default: `/var/lib/openpanel/snapshots`)
- Host ID persisted to `<snapshots_root>/host-id` for cross-run stability
- Restore delegates to existing `BackupService::restore` pipeline
- CLI commands use `snapshot_caller()` helper (Owner role required)
