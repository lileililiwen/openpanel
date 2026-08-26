# Add Server snapshot & migration — Tasks

## 1. Testing

- [x] 1.1 Unit: `SnapshotManifest` validation — unknown
      `manifest_schema` → `SchemaTooNew`; entry with mismatched
      sha256 length → invalid; empty entries list rejected for full
      snapshots.
- [x] 1.2 Unit: preflight on a manifest from "another host" emits a
      same-host=false report; on the local host's own id emits the
      same-host warning; version skew produces a warning, not an error.
- [x] 1.3 Unit: confirmation token is single-use — second restore with
      the same token fails before any write.
- [x] 1.4 Property (`mod prop`): for arbitrary valid manifests (≥100
      cases), bundling then hashing reproduces identical manifest
      bytes and every listed hash matches its entry (round-trip).
- [x] 1.5 Integration (`tests/integration/server_snapshot.rs`):
      create snapshot on seeded host → artifact exists with 4 entry
      kinds; inspect SSL entry → ciphertext layout only (assert no
      `BEGIN` PEM marker anywhere in the bundle); restore into a fresh
      `TestDb` + temp docroot → site list, database list, cert
      metadata equal to source.
- [x] 1.6 Integration: tamper one byte of a site entry → restore
      aborts at that resource, result lists applied resources so far,
      exit non-zero.
- [x] 1.7 Integration: migrate round-trip between two in-memory hosts
      sharing a temp offsite target directory.
- [x] 1.8 CLI E2E: `cli_server_snapshot_create_then_list`,
      `cli_server_restore_requires_confirm`.
- [ ] 1.9 Web: Snapshots page renders at 360/768/1280 px; restore
      wizard shows preflight warnings; screenshots in PR.

## 2. Domain

- [x] 2.1 Add `ServerSnapshot`, `SnapshotManifest`, `SnapshotError`,
      pure validators under `crates/openpanel-domain/src/backups/`.

## 3. Application

- [x] 3.1 Bundling writer reusing `BackupResource::*` writers;
      zstd+tar packaging; per-entry sha256.
- [x] 3.2 Preflight + single-use confirm tokens; ordered restore with
      per-resource audit events.
- [x] 3.3 `SnapshotImporter` implementing the migration-importers
      driver contract; offsite-target export/import glue.
- [ ] 3.4 Cron job type for scheduled snapshots; plan-aware retention
      pruning with audit.

## 4. Adapters and UI

- [x] 4.1 REST routes `/api/v1/server/snapshots*`, `/server/migrate`.
- [x] 4.2 CLI `openpanel server {snapshot,snapshots,restore,migrate}`.
- [ ] 4.3 Web Server → Snapshots page.

## 5. Validation

- [x] 5.1 `cargo test --workspace` twice, identical results.
- [ ] 5.2 `make check` clean.
- [ ] 5.3 Smoke-test: snapshot a dev host, restore into a container,
      curl the restored site over HTTPS.
- [ ] 5.4 Archive with `openspec archive add-server-snapshot-migration`.
