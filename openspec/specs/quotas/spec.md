# quotas Specification

## Purpose

Adds per-user / per-site resource limits (disk, bandwidth,
inodes, max file size, optional CPU shares) so the panel can host
multiple resellers without one account filling the disk or
saturating the link. The model exposes soft/hard pairs with grace
windows and emits audit events when the soft or hard thresholds
are crossed.

## Requirements

### Requirement: Quota Policy Aggregate

The quotas context SHALL model a `QuotaPolicy` carrying: an id, a `subject_kind` of `User | Site`, a `subject_id`, a `disk` `QuotaLimit` (soft, hard, grace), a `bandwidth` `QuotaLimit`, an `inodes` `QuotaLimit`, an optional `max_file_size_bytes`, an optional `cpu_shares`, and `created_at` / `updated_at`. The aggregate MUST reject policies whose hard limit is zero and MUST reject policies whose soft limit exceeds the hard limit.

#### Scenario: Policy validates limits

- **WHEN** a policy is created with `disk_hard = 0` OR `disk_soft > disk_hard`
- **THEN** the constructor returns `QuotaError::InvalidLimit`.

#### Scenario: Single policy per subject

- **WHEN** an Owner submits a policy for a subject that already has one
- **THEN** the existing policy is replaced in place rather than a duplicate row.

### Requirement: Soft / Hard Threshold Detection

The `QuotaUsage` value object SHALL record `over_soft: Set<QuotaDimension>` and `over_hard: Set<QuotaDimension>` based on the current usage against the policy. The `QuotaService::sample` SHALL persist the sample and emit an audit event when either set is non-empty.

#### Scenario: Soft threshold triggers a warning

- **WHEN** a sample has `disk_used_bytes > disk_soft_bytes` but `disk_used_bytes <= disk_hard_bytes`
- **THEN** the audit log records `QuotaSoftLimitReached{subject_id, over_soft=[disk]}`.

#### Scenario: Hard threshold blocks writes

- **WHEN** a sample has `disk_used_bytes > disk_hard_bytes`
- **THEN** the audit log records `QuotaHardLimitReached{subject_id, over_hard=[disk]}` with outcome `Failure`.

### Requirement: Sample Ingestion and Latest Usage

The service SHALL expose `sample(policy, disk_used, inodes_used, bandwidth_used, actor)` which inserts a new `QuotaUsage` row and `latest_usage(subject_id)` which returns the most recent row.

#### Scenario: Sample round-trips

- **WHEN** a sample is inserted for a user
- **THEN** `latest_usage(user_id)` returns that row by `sampled_at DESC`.

### Requirement: Enforcement Surface (Ports)

The kernel-level enforcement of disk / bandwidth / CPU limits is owned by the follow-on `add-multi-host-agent` change. The `quotas` bounded context owns the typed ports (`QuotaRepository`) and the policy model; enforcers implement those ports and may report `EnforcerUnavailable` when the host is not prepared (e.g. the FS is not mounted with `usrquota`).
