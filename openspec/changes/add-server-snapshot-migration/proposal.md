# Add Server snapshot & migration

## Why

1Panel ships whole-system snapshots with restore/migration and BaoTa
ships whole-machine migration; both use them as the primary "move my
server" story. OpenPanel has all the building blocks — panel-metadata
backup (`BackupResource::PanelMetadata`), per-resource site/database
backups, offsite targets with KEK handling, and a tar+JSON importer
driver — but no single whole-server bundle, no restore-into-fresh-host
flow, and no host-to-host migrate path. Operators replacing a VPS must
hand-stitch resources today.

## What Changes

- **Server snapshot**: one artifact bundling panel metadata + sites +
  databases + SSL certificate material (keys stay AES-256-GCM
  ciphertext) + module configs, described by a versioned JSON
  manifest.
- **Restore preflight**: schema-version compatibility check and
  host-id mismatch warning before anything is written.
- **Host-to-host migration**: export to an attached offsite target,
  import on the new host through the existing migration-importer
  driver contract.
- **Scheduling/retention**: snapshots run as cron jobs with plan-aware
  retention.
- Surfaces: API routes, `openpanel server {snapshot,restore,migrate}`
  CLI, web Server → Snapshots page.

## Capabilities

### Modified Capabilities

- `backups`: add the server-snapshot resource kind, restore preflight,
  and migrate flow on top of existing resource backups.

## Impact

- Domain: `ServerSnapshot` aggregate, `SnapshotManifest{schema_version,
  host_id, created_at, entries[]}`, `SnapshotError`.
- App: extends `crates/openpanel-app/src/backups/service.rs`
  (existing `BackupResource::*` writers) with a bundling writer;
  reuses offsite-target adapter contract for transport; new
  `SnapshotImporter` implementing the migration-importers
  `DriverContract`; cron integration via existing job scope.
- API/CLI/web: `/api/v1/server/snapshots*`, CLI subcommands, web page.
- Security: private keys remain ciphertext inside the bundle; manifest
  carries no secrets; restore requires explicit confirmation token.
- Coupling: backups (core), offsite-backup-targets, cron,
  migration-importers, ssl (key ciphertext), hosting-plans (retention).

## Non-goals

- No live block-level disk imaging (provider snapshots cover that).
- No cross-panel import — that stays in migration-importers.
- No automatic DNS cutover during migrate.
