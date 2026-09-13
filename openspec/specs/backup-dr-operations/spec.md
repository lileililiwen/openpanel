# backup-dr-operations Specification

## Purpose
TBD - created by archiving change complete-backup-dr-and-migration-operations. Update Purpose after archive.
## Requirements
### Requirement: Backup Health Is Actionable

Each backup plan and remote target MUST expose last success, next run, age,
retention, verification state, and safe recovery guidance.

#### Scenario: Stale plan

- **WHEN** a plan has no successful run within its expected interval
- **THEN** the health surface marks it stale and links to logs/retry/configuration.

### Requirement: Restore Drills Prove Recoverability

Restore drills MUST use the production restore pipeline in an isolated sandbox,
persist per-assertion results, retain bounded history, and tear down on success
or failure.

#### Scenario: Corrupt database dump

- **WHEN** a drill cannot apply a database dump
- **THEN** it reports a stable failed assertion, dispatches configured alerting,
  and leaves no sandbox resources.

### Requirement: Restore Is Preflighted and Scoped

Every restore MUST show compatibility, collisions, resource scope, and
estimated impact before issuing a single-use confirmation token.

#### Scenario: Restore without confirmation

- **WHEN** execution is requested without a valid preflight token
- **THEN** no database, filesystem, or configuration mutation occurs.

### Requirement: Host Migration Is Verifiable

Host migration MUST export a versioned integrity-checked bundle, provide a
bootstrap path, and verify the destination resource inventory and audit trail.

#### Scenario: Migration round-trip

- **WHEN** a source bundle is imported to a fresh compatible host
- **THEN** sites, databases, certificates, and safe metadata match the source.

