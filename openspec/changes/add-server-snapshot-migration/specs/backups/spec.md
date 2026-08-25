## ADDED Requirements

### Requirement: Server Snapshot Bundle

The system SHALL produce a single server-snapshot artifact bundling
panel metadata, sites, databases, and SSL certificate material
described by a versioned JSON manifest with per-entry SHA-256 hashes.
Private keys SHALL remain AES-256-GCM ciphertext inside the bundle and
the master key SHALL never be included.

#### Scenario: Bundle created

- **WHEN** an Admin creates a full snapshot on a host with one site,
        one database, and one issued certificate
- **THEN** the artifact contains a schema-versioned manifest listing
          all four resource kinds with matching hashes.

#### Scenario: Keys stay encrypted

- **WHEN** any snapshot artifact is inspected
- **THEN** SSL key entries contain ciphertext in the documented
          layout, never PEM plaintext, and the manifest contains no
          secret material.

### Requirement: Restore Preflight

Restoring a snapshot SHALL require a preflight step that checks
manifest-schema compatibility, warns on panel-version skew and
same-host restores, lists name collisions, and issues a single-use
confirmation token; restore without that token SHALL be refused before
any mutation.

#### Scenario: Schema too new

- **WHEN** preflight sees a manifest schema newer than supported
- **THEN** it fails with `SnapshotError::SchemaTooNew` and offers no
          token.

#### Scenario: Confirmation gate

- **WHEN** restore is called without the token from a completed
          preflight
- **THEN** nothing is written to disk or database.

### Requirement: Ordered Restore

Restore SHALL verify each entry's hash, then apply resources in
dependency order (databases, sites, certificates, metadata) and record
an audit event per applied resource plus a completion event.

#### Scenario: Tampered entry aborts

- **WHEN** an entry's bytes do not match its manifest hash
- **THEN** restore fails before applying that resource and previously
          applied resources are reported in the result.

### Requirement: Host-to-Host Migration

The system SHALL support migrating by exporting a full snapshot to an
attached offsite target and importing it on a fresh host through the
migration-importer driver contract, emitting a bootstrap command for
the new host.

#### Scenario: Migrate round-trip

- **WHEN** an operator runs migrate against target T, then runs the
          bootstrap command on a fresh host attached to T
- **THEN** the fresh host reports the same sites, databases, and
          certificates as the source, and audit events exist on both
          hosts.

### Requirement: Scheduled Snapshots and Retention

Snapshots SHALL be schedulable through the existing cron capability
and retention SHALL follow the account's hosting plan; pruning beyond
retention SHALL be audited.

#### Scenario: Retention enforced

- **WHEN** a plan allows 5 snapshots and a scheduled run creates the
          6th
- **THEN** the oldest snapshot is pruned and a `SnapshotPruned` audit
          event records which.
