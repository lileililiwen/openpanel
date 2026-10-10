# backups Specification

## ADDED Requirements

### Requirement: Backup Preset Binding

Backup-Site, Backup-DB, and Backup-Dir cron presets MUST create their
schedules as `BackupService` plans bound to the triggering job, reusing the
existing plan/run lifecycle without changing its transitions.

#### Scenario: Preset creates bound plan

- **WHEN** an authorized user creates a Backup-Site preset
- **THEN** a backup plan exists with the job's schedule and runs through
  the standard plan/run lifecycle.

### Requirement: Backup Destination Choices

Each backup preset MUST offer Backup-to choices — Local, FTP, S3, GDrive —
mapped onto the existing `BackupTargetKind` + offsite credential contract.
Unknown credentials MUST refuse attachment before any mutation.

#### Scenario: Destination maps to target kind

- **WHEN** a preset selects S3 with a valid credential
- **THEN** the plan's remote config carries the mapped target kind and the
  credential id.

### Requirement: Retain-Latest-N And Exclusion Rules

Each backup preset MUST offer Retain-latest-N (mapped to
`retention_copies`) and exclusion rules (path patterns stored as plan
filters). Retention pruning MUST follow the existing retention behavior.

#### Scenario: Retention enforced

- **WHEN** a preset keeps 5 copies and a run creates the 6th
- **THEN** the oldest copy is pruned per the existing retention rule.
