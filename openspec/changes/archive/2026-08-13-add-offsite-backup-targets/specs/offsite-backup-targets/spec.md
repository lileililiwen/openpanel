## Purpose

Implements the remote storage adapters and credential / KEK
management introduced structurally by the backups refinement.
Offsite targets turn backups into disaster-recovery-grade data
by replicating artefacts to S3, S3-compatible (Wasabi), B2, or
rsync destinations under AES-256-GCM encryption with a
passphrase-derived KEK usable in cold-restore scenarios.

# offsite-backup-targets Specification

## Requirements

### Requirement: Target Adapter

The offsite-backup-targets context SHALL define a `BackupTargetAdapter` trait with `put`, `get`, `list`, `delete`, and `test` operations. Concrete adapters SHALL be provided for `S3`, `Wasabi`, `B2`, and `Rsync+Ssh`. The `test` operation MUST perform a connectivity check (HEAD/LIST-equivalent for object stores; a `ssh -o BatchMode=yes` for rsync) and report `reachable` plus `latency_ms` without leaking credentials.

#### Scenario: Adapter exposes typed errors

- **WHEN** `put` fails with a credentials error
- **THEN** the error is a typed `BackupTargetError::Unauthorized{redacted_reason}` and the panel audit logs the redacted reason only.

#### Scenario: Test reports reachable

- **WHEN** an Owner runs `POST /backups/remote/test` against a valid S3 credential
- **THEN** the response is `{ reachable: true, latency_ms: <int> }`.

### Requirement: Encrypted Credential Storage

The system SHALL encrypt every credential blob at rest with AES-256-GCM under a passphrase-derived KEK. The KEK is wrapped with the master key so the panel can decrypt without the passphrase in normal operation, while a cold backup remains decryptable with just the passphrase plus the master-key fingerprint. Plaintext credentials SHALL NEVER appear in any log, error, audit, API, CLI, or web response.

#### Scenario: Credential create returns metadata only

- **WHEN** an Owner posts a new S3 credential
- **THEN** the response is `{ id, label, kind }` and never contains the access/secret key.

#### Scenario: Tampered blob fails to decrypt

- **WHEN** an attacker modifies one byte of the encrypted credential
- **THEN** `BackupCredential::decrypt` returns `BackupTargetError::CryptoFailure` and the audit `CredentialDecryptFailed` records only the credential id.

#### Scenario: Passphrase-only cold restore

- **WHEN** the panel database is restored from a cold backup on a fresh host
- **THEN** the KEK is re-derivable with `passphrase + master_key_fingerprint`, the credential blob decrypts, and remote-config resumes.

### Requirement: Plan Remote Config Attachment

The system SHALL let an Owner attach a `RemoteTargetConfig` to a plan via `POST /api/v1/backups/plans/{id}/remote-config`. A remote-config attachment refuses to be created if the credential is missing, disabled, or already attached elsewhere. Removing a remote-config requires a fresh destructive confirmation.

#### Scenario: Attach succeeds

- **WHEN** an Owner attaches a credential and prefix to a plan
- **THEN** subsequent runs for that plan write artefacts to `(bucket, prefix)` and the manifest records the target's id and prefix.

#### Scenario: Duplicate attachment rejected

- **WHEN** the same credential is already attached to another plan
- **THEN** the request is rejected with `CredentialAlreadyAttached`.

#### Scenario: Removing a remote-config requires confirmation

- **WHEN** `DELETE /plans/{id}/remote-config` is called without `confirmed_at`
- **THEN** the request is rejected with 400 `missing_confirmation`.

### Requirement: Offsite-Encrypted Storage Format

Every artefact uploaded to a remote target SHALL be wrapped in a header `#OPBK2\n` followed by a version line, the AES-GCM nonce, the wrapped KEK id, and the ciphertext. The format is reversible on a hot host (panel decrypts with the master-key wrapping) and on a cold host (panel re-derives the KEK from passphrase + master fingerprint).

#### Scenario: Header written

- **WHEN** an artefact is uploaded to S3
- **THEN** the body begins with `#OPBK2\n`, then `version=2`, `nonce=<hex>`, `kek_id=<id>`, then the ciphertext.

#### Scenario: Hot restore

- **WHEN** the panel downloads an artefact and unwraps it on a host with the master key
- **THEN** the unwrap succeeds and the original bytes are produced.

#### Scenario: Cold restore

- **WHEN** the same artefact is restored on a fresh host with the operator's passphrase
- **THEN** the panel re-derives the KEK, decrypts the wrapping, decrypts the body, and returns the original bytes.
