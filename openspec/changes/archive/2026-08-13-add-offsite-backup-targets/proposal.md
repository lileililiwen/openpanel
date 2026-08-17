# Add offsite backup targets

## Why

`refine-backups-with-resource-restore-and-policies` introduced a
typed `BackupTargetKind` (`Local`, `OffsiteS3`, `OffsiteRsync`,
`OffsiteB2`, `OffsiteWasabi`) on every plan but did not implement
the storage adapters. A backup that lives only on the host it's
backing up is not a backup — it dies with the host. cPanel's
"Backup Configuration" and Baota's remote-sync feature both let
operators push archives to S3, rsync to a remote, or ship to
B2 / Wasabi. This change implements the remote storage adapters
and the credential / encryption-at-rest contract.

## What Changes

- New `BackupTargetAdapter` trait for each `BackupTargetKind`.
- Concrete adapters for S3, S3-compatible (Wasabi), B2, rsync+ssh.
- New bounded context `offsite-backup-targets` carrying
  `BackupCredential` (encrypted) and the adapter registry.
- New SQLite tables: `backup_credentials` (encrypted),
  `backup_remote_targets` (per plan, encrypted destination config).
- New endpoints: `POST/GET/DELETE /api/v1/backups/credentials`,
  `POST /api/v1/backups/plans/{id}/remote-config` to attach a
  target.
- New key material policy: panel master key encrypts at rest; a
  separate **KEK** derived for backups enables off-host decryption
  with a passphrase in disaster-recovery scenarios.

## Capabilities

### New Capabilities

- `offsite-backup-targets`: remote storage adapters and
  credential management.

## Impact

- Domain: `BackupTargetAdapter`, `BackupCredential`,
  `RemoteTargetConfig`, `KekRef`.
- App: `BackupTargetRegistry`, `KekManager`,
  `BackupUploadService`.
- API/CLI/web: `/api/v1/backups/credentials/*`,
  `/api/v1/backups/plans/{id}/remote-config`; CLI
  `openpanel backups remote {create,list,test}`.
- Crypto: KMS-style KEK derived via Argon2id from the
  operator-provided passphrase + master key fingerprint; AES-256-GCM
  payload encryption under the KEK.
