## Context

Site files, MySQL data, TLS metadata, and panel state span filesystem and databases. A useful backup needs a manifest and coordinated snapshots; simply compressing directories is neither verifiable nor safely restorable.

## Goals / Non-Goals

**Goals:** local scheduled/on-demand backups, selectable resources, checksums, retention, granular restore, progress, and failure-safe staging.

**Non-Goals:** bare-metal OS imaging, live block snapshots, cross-version migration, remote destinations in v1, or exporting TLS private keys in plaintext.

## Decisions

1. Persist `BackupPlan`, `BackupRun`, `BackupArtifact`, and a versioned manifest. Each artifact records resource identity, size, SHA-256, and format version.
2. Stream site archives and `mysqldump` output into a run-specific staging directory, fsync, verify, then atomically rename into the destination. Partial runs never enter the restorable set.
3. Encrypt secret-bearing metadata with the existing master-key envelope. Never place plaintext database passwords or TLS keys in manifests or logs.
4. Restore is an asynchronous job with preflight space/version/checksum checks. Default conflict policy is `fail`; overwrite requires an Owner confirmation token and creates a safety backup where feasible.
5. Backup schedules reference Cron jobs rather than building a second scheduler. Retention runs only after a successful finalized backup.

## Risks / Trade-offs

- Cross-resource snapshots are not globally atomic -> record per-resource timestamps and consistency level in the manifest.
- Restore can destroy newer data -> fail by default, preview changes, explicit overwrite, audit, and safety copy.
- Archives can exhaust disk -> preflight estimates, streaming, quotas, and cleanup of abandoned staging.

## Migration Plan

Create empty metadata tables and a configured backup root. Existing resources are unaffected until a plan or manual run is created. Rollback stops jobs and leaves artifacts readable.
