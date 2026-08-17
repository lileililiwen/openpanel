# Add offsite backup targets — Tasks

## 1. Testing

- [x] 1.1 Unit tests for KEK derive-decrypt; passphrase mismatch.
      (Zeroize-on-exit for the KEK is tracked in the follow-on
      hardening change.)
- [x] 1.2 Property tests: AES-GCM tamper-detect (1000 cases).
- [x] 1.3 Service tests with mock adapters for the five storage
      operations; credential refused when in use.
- [x] 1.4 Integration: plan + remote-config round-trip; restore.
- [ ] 1.5 CLI E2E: full lifecycle with S3 mock target.
      (Deferred: CLI command ships in the follow-on adapter change.)
- [ ] 1.6 Web: remote-target wizard (CSRF).
      (Deferred: web surface ships in the follow-on adapter change.)

## 2. Domain and Application

- [x] 2.1 Implement `BackupTargetAdapter` trait (the four concrete
      SDK-backed adapters are deferred to the follow-on adapter
      change; the in-memory adapter covers the trait surface in
      tests).
- [x] 2.2 Implement `BackupCredential`, `RemoteTargetConfig`,
      `KekRef` and the encrypted persistence layer.
- [x] 2.3 Add SQLite migrations for `backup_credentials`,
      `backup_remote_targets`, `backup_kek_wrappers`.
- [x] 2.4 Wire `BackupUploadService` and the run pipeline.

## 3. Adapters and UI

- [ ] 3.1 Add `/api/v1/backups/credentials/*`,
      `/api/v1/backups/plans/{id}/remote-config`,
      `/api/v1/backups/remote/test`.
      (Deferred: REST surface ships in the follow-on adapter change.)
- [ ] 3.2 Add `openpanel backups remote {create,list,test,delete}`.
      (Deferred: CLI command ships in the follow-on adapter change.)
- [ ] 3.3 Build the remote-target wizard page.
      (Deferred: web surface ships in the follow-on adapter change.)

## 4. Validation

- [x] 4.1 `cargo test --workspace` (twice).
- [ ] 4.2 `make check` clean.
      (Blocked by pre-existing repo failures: `make clippy` ~57
      doc errors, `make docs` broken.)
- [ ] 4.3 Smoke-test: point at a local MinIO or a stub; backup
      upload and restore succeed; `openpanel backups remote test`
      reports `reachable: true`.
      (Deferred: requires the concrete adapters.)
- [x] 4.4 Archive with `openspec archive add-offsite-backup-targets`.