# backups Specification

## Purpose
TBD - created by archiving change add-backup-restore. Update Purpose after archive.
## Requirements
### Requirement: Backup Plans and Runs

The system SHALL let authorized callers create, update, enable, disable, delete, and run plans selecting owned sites, managed databases, and panel metadata, with schedule and retention. A run SHALL use isolated staging, stream resource data, produce a versioned manifest, and become `Completed` only after atomic finalization.

#### Scenario: Scheduled backup succeeds

- **WHEN** Cron triggers a plan selecting one site and one database
- **THEN** a finalized run contains both artifacts and a manifest with checksums, sizes, timestamps, and consistency metadata

#### Scenario: Resource capture fails

- **WHEN** any selected required resource cannot be captured
- **THEN** the run is `Failed`, is not offered for restore, and its staging data is cleaned safely

### Requirement: Integrity and Secret Safety

Every artifact SHALL have a SHA-256 checksum verified after creation and before restore. Manifests, logs, APIs, CLI output, and web pages MUST NOT contain plaintext credentials, session tokens, master keys, or TLS private keys; secret-bearing data SHALL remain encrypted under the master key.

#### Scenario: Corrupt artifact

- **WHEN** verification calculates a checksum different from the manifest
- **THEN** the run is marked corrupt and restore is blocked

### Requirement: Restore Lifecycle

Restore SHALL be an asynchronous previewable job with authorization, version/space/checksum preflight, per-resource selection, progress, and terminal result. Conflicts SHALL fail by default; overwrite SHALL require an Owner's short-lived confirmation token and SHALL be audited.

#### Scenario: Safe restore into an empty target

- **WHEN** an authorized caller restores verified site files to an absent target
- **THEN** files are staged, permissions validated, atomically installed, and the restore completes

#### Scenario: Conflict without overwrite

- **WHEN** target data exists and overwrite was not explicitly confirmed
- **THEN** preflight reports the conflicts and changes no target data

### Requirement: Backup Surfaces and Retention

REST, CLI, and `/backups` web surfaces SHALL expose plans, runs, progress, verification, restore preview/start, and delete. Users SHALL access only owned resources; Admins SHALL not restore Owner resources. Retention SHALL delete only finalized artifacts outside policy and SHALL preserve active or pinned runs.

#### Scenario: Retention after success

- **WHEN** a plan retaining three copies finalizes a fourth successful run
- **THEN** the oldest unpinned completed run and its artifacts are deleted after the new run is verified

#### Scenario: Unauthorized download

- **WHEN** a User requests an artifact belonging to another owner
- **THEN** the system returns forbidden without revealing its path or existence details
