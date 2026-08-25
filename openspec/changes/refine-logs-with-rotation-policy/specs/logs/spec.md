## ADDED Requirements

### Requirement: Rotation Policies

The system SHALL let Admins define one validated rotation policy per
authorized source class (site access/error, managed service, panel)
with max age (1–365 days), max size (1–4096 MiB), retained generations
(1–52), and compression toggle.

#### Scenario: Policy persisted

- **WHEN** an Admin sets site-access rotation to 30 days / 100 MiB /
        8 generations / compressed
- **THEN** the policy is stored, rendered to the managed drop-in, and
          returned by subsequent reads.

#### Scenario: Out-of-bounds rejected

- **WHEN** a policy sets max age 400 days
- **THEN** validation fails with `LogsError::PolicyBounds` and no file
          is modified.

### Requirement: Managed Drop-In Application

Rendered rotation configuration SHALL be written as panel-owned
managed blocks in the system logrotate drop-in directory, atomically,
preserving foreign content outside the block; the system SHALL detect
when on-disk state drifts from the stored policy and surface it.

#### Scenario: Drift surfaced

- **WHEN** an operator edits the generated drop-in out of band
- **THEN** the policy read model reports drift until re-applied.

### Requirement: Manual Rotation

Admins SHALL trigger immediate rotation per source class through the
system logrotate binary; when the binary is absent, mutation surfaces
SHALL return 503 `logrotate_unavailable`.

#### Scenario: Force rotate

- **WHEN** an Admin requests manual rotation for the panel class
- **THEN** logrotate runs forced against that class's config and the
          outcome is audited.

### Requirement: Policy Surfaces and Audit

Policies SHALL be manageable via API, CLI, and web; every change and
manual rotation SHALL be audited without log content in events.

#### Scenario: Audit hygiene

- **WHEN** any policy mutation occurs
- **THEN** the audit event names the source class and changed fields
          only.
