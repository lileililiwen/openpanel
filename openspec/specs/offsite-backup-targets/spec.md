# offsite-backup-targets Specification

## Purpose

The offsite-backup-targets bounded context lets operators attach
remote storage to backup plans. A backup that lives only on the
host it is backing up dies with the host, so plans must be able to
push archives to S3 / S3-compatible (Wasabi), Backblaze B2, or an
rsync-over-SSH endpoint. The change ships the `BackupTargetAdapter`
contract, the encrypted `BackupCredential` / `RemoteTargetConfig` /
`KekRef` model, a KEK (key-encryption-key) derivation and wrapping
scheme, the SQLite persistence layer, and the `BackupUploadService`
application service. Concrete S3 / Wasabi / B2 / rsync adapters are
thin layers over the existing Rust SDKs and ship in the follow-on
adapter changes.

## Requirements

### Requirement: Adapter Contract

The bounded context SHALL model a `BackupTargetAdapter` trait with
an associated `Error` and five async operations: `put`, `get`,
`list`, `delete`, and `test`. `delete` of a missing key MUST NOT be
an error; `test` probes reachability and credentials without
writing data.

#### Scenario: Upload through the adapter

- **WHEN** the service uploads bytes to a key on a reachable adapter
- **THEN** the object appears in `list` and `get` returns the bytes.

### Requirement: Credential Model

The bounded context SHALL model a `BackupCredential` carrying an id,
a `CredentialKind` (`s3`, `wasabi`, `b2`, `rsync`), a label (1..=64
chars), and an opaque encrypted secret in `nonce_hex:ciphertext_hex`
form. The plaintext payload is encrypted under the KEK before it
touches persistence, and listing MUST NOT echo secrets.

#### Scenario: Secret never echoed

- **WHEN** a credential is listed
- **THEN** the response carries only id, kind, label, and timestamps.

#### Scenario: Invalid label rejected

- **WHEN** a credential is built with an empty or overlong label
- **THEN** construction fails with `InvalidLabel`.

### Requirement: Remote Target Config

The bounded context SHALL model a `RemoteTargetConfig` per plan,
carrying the plan id, a `credential_id`, an object-key `prefix`
(non-empty, ≤ 1024 chars, MUST NOT start with a slash), and an
optional cron `schedule`. The config composes an object key by
joining the prefix and the artifact name with a single slash.

#### Scenario: Leading slash rejected

- **WHEN** a remote target is built with a prefix starting with `/`
- **THEN** construction fails with `InvalidPrefix`.

### Requirement: KEK Management

The KEK SHALL be derived with Argon2id from the operator passphrase
salted by the hex master-key fingerprint, so a cold restore can
re-derive the same key without the master key. Payloads are
encrypted with AES-256-GCM under the KEK; the KEK is additionally
wrapped with the master key into `backup_kek_wrappers` so the panel
can decrypt without the passphrase during normal operation. A
passphrase mismatch MUST derive a different KEK, and any tampering
with the nonce or ciphertext MUST fail the AES-GCM tag check.

#### Scenario: Passphrase mismatch

- **WHEN** two different passphrases derive KEKs with the same salt
- **THEN** the KEKs differ.

#### Scenario: Tamper detected

- **WHEN** a single byte of the ciphertext or nonce is altered
- **THEN** `decrypt_payload` returns `CredentialDecrypt`.

### Requirement: Credential Lifecycle

The service SHALL provide `create_credential` (encrypting the
payload at rest and recording `BackupCredentialCreated`),
`list_credentials`, `delete_credential`, and `decrypt_credential`.
Deleting a credential that is attached to a plan's remote config
MUST be refused with `CredentialInUse` and recorded as
`BackupCredentialInUseRejected`.

#### Scenario: Credential refused when in use

- **WHEN** a credential is attached to a plan and a delete is attempted
- **THEN** the service returns `OffsiteBackupError::CredentialInUse`
  and records `BackupCredentialInUseRejected`.

### Requirement: Remote Target Attachment

The service SHALL provide `attach_remote_target` which refuses a
config whose credential does not exist, records
`BackupRemoteTargetAttached`, and stores (or replaces) the config.
The service SHALL provide `test_remote` which runs the adapter's
`test`, measures round-trip latency, and records
`BackupRemoteTested` with `reachable` and `latency_ms`.

#### Scenario: Attach requires existing credential

- **WHEN** a remote target config references an unknown credential
- **THEN** the service returns `OffsiteBackupError::CredentialNotFound`.

## Release Notes

This change ships the domain model, KEK crypto, repository, and the
upload service. The REST endpoints (`/api/v1/backups/credentials/*`,
`/api/v1/backups/plans/{id}/remote-config`, `/api/v1/backups/remote/test`),
the CLI (`openpanel backups remote {create,list,test,delete}`), and
the concrete S3 / Wasabi / B2 / rsync adapters ship in the follow-on
adapter and adapter-surface changes.