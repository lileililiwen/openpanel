# Add Server snapshot & migration — Design

## Explore & Reuse

- `crates/openpanel-app/src/backups/service.rs:354–384` — existing
  `BackupResource::{PanelMetadata, Site, Database}` writers; the
  snapshot bundles their artifacts instead of reimplementing them.
- `openspec/specs/offsite-backup-targets/spec.md` — Adapter Contract +
  KEK Management: snapshot export/import rides the same adapters and
  key handling; no new credential model.
- `openspec/specs/migration-importers/spec.md` — Driver Contract +
  tar-with-JSON-manifest reference driver: the restore side implements
  the driver interface so future cross-panel sources plug in.
- `openspec/specs/cron/spec.md` — Job Scope for scheduled snapshots.
- `openspec/specs/hosting-plans/spec.md` — Plan Resolver for
  retention limits.
- Audit: `AuditService` events `SnapshotCreated`, `SnapshotRestored`,
  `MigrateCompleted`.

## Manifest (versioned, secret-free)

```json
{
  "manifest_schema": 1,
  "created_at": "...",
  "source_host_id": "…",
  "panel_version": "0.1.0",
  "entries": [
    {"kind": "panel_metadata", "path": "meta.tar.zst", "sha256": "…"},
    {"kind": "site",     "ref": "example.com", "path": "sites/example.com.tar.zst", "sha256": "…"},
    {"kind": "database", "ref": "wp1",         "path": "db/wp1.sql.zst",          "sha256": "…"},
    {"kind": "ssl_keys", "ref": "example.com", "path": "ssl/example.com.enc",     "sha256": "…"}
  ]
}
```

Bundle layout: `manifest.json || entries…`, zstd-compressed tar;
`ssl_keys` entries are the existing AES-256-GCM ciphertext blobs —
the master key never travels.

## Flows

```
create_snapshot(scope?):
   for resource in resolve(scope): reuse BackupResource writers -> temp artifacts
   write manifest (sha256 each entry); bundle; optional push to offsite target
   audit SnapshotCreated{bytes, entries}

preflight(bundle):
   manifest_schema supported? else SnapshotError::SchemaTooNew
   target panel_version >= source minor? warn
   source_host_id == local? warn "same-host restore"
   name collisions listed for confirmation token

restore(bundle, confirm_token):
   require token from preflight; verify sha256 per entry
   order: databases -> sites -> ssl -> metadata; audit SnapshotRestored

migrate(target_ref):
   create_snapshot(all) -> offsite target -> print one-line bootstrap
   command for the new host (pulls via importer driver)
```

## Endpoints / CLI / Web

```
POST /api/v1/server/snapshots            {scope?}
GET  /api/v1/server/snapshots[/{id}]
POST /api/v1/server/snapshots/{id}/preflight
POST /api/v1/server/snapshots/{id}/restore {confirm_token}
POST /api/v1/server/migrate               {target}
CLI: openpanel server {snapshot,snapshots,restore,migrate}
Web: Server → Snapshots (list, create, restore wizard w/ preflight report)
```

## Layering

Domain: pure aggregate/manifest validation. App: bundler, preflight,
importer driver impl, cron glue. Adapters unchanged in shape.
