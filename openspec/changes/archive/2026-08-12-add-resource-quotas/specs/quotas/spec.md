## ADDED Requirements

### Requirement: Quota Policy

The system SHALL let an Owner set per-user or per-site `QuotaPolicy` records covering disk (soft, hard, grace), bandwidth (soft and hard MB per month), inodes (soft, hard), and an optional max file size. Invalid combinations (soft > hard, negative values, non-integer MB) MUST be rejected.

#### Scenario: Set a per-site disk quota

- **WHEN** an Owner posts `{ disk_soft_mb: 100, disk_hard_mb: 200, disk_grace_days: 7 }` for a site
- **THEN** the policy is persisted and the disk enforcer is applied to the site's filesystem path.

#### Scenario: Reject soft > hard

- **WHEN** a request sets `disk_soft_mb: 500, disk_hard_mb: 100`
- **THEN** the panel returns 422 and the policy is not persisted.

### Requirement: Disk Enforcement

The panel SHALL apply disk quotas via the kernel `quotactl` interface against a quota-capable filesystem. A filesystem mounted without `usrquota`/`grpquota` is reported as `unsupported` and the panel returns 503 with a clear remediation step. Writes that exceed the hard limit MUST be rejected at the file manager layer with a typed `QuotaExceeded` error.

#### Scenario: Hard limit reached

- **WHEN** the user attempts a write that would push disk usage over `disk_hard_mb`
- **THEN** the kernel returns `EDQUOT`, the file manager returns a typed `QuotaExceeded` to the API, and an `AlertFired` event is recorded.

#### Scenario: Soft limit reached

- **WHEN** usage crosses `disk_soft_mb` but not the hard limit
- **THEN** the grace timer starts and a warning alert is recorded; writes continue until the grace window elapses.

### Requirement: Bandwidth Enforcement

The panel SHALL measure per-UID egress bytes and shape bandwidth via `tc` per-UID classes. The first second of each UTC month resets the counter. Crossing the soft threshold records a warning alert; crossing the hard threshold sets the class rate-limit to zero (egress only; the API and SSH remain reachable).

#### Scenario: Soft warning

- **WHEN** egress in the current month reaches 90% of `bandwidth_soft_mb_per_month`
- **THEN** the panel records a warning event and the quota-usage record is updated.

#### Scenario: Hard block

- **WHEN** egress exceeds `bandwidth_hard_mb_per_month`
- **THEN** the panel changes the user's tc class to rate 0 and audits `BandwidthBlocked`; new egress is dropped at the kernel.

### Requirement: Inode Enforcement

The panel SHALL count inodes under the user's home tree via a checkpointed walker, and SHALL set `QIF_INODES` soft/hard limits via `quotactl`. Writes that exceed the hard inode limit return `EDQUOT`.

#### Scenario: Inode hard limit

- **WHEN** a write would push the inode count over `inode_hard`
- **THEN** the kernel returns `EDQUOT` and the file manager surfaces the error.

### Requirement: Quota Sampler and Monitoring Integration

A periodic quota sampler SHALL emit usage rows to the monitoring time series and SHALL fire `AlertFired` events on soft/hard crossings using the existing hysteresis model. The notification dispatcher (separate capability) consumes these events.

#### Scenario: Alert hysteresis

- **WHEN** a soft threshold is crossed and the previous sample was below it
- **THEN** exactly one warning alert is recorded.

#### Scenario: Recovery

- **WHEN** usage drops below the soft threshold after a warning
- **THEN** a `QuotaRecovered` event is recorded and no further warning fires until the next crossing.

### Requirement: Quota Surfaces

REST, CLI, and web surfaces SHALL support policy get/set, usage read, and disable. Browser mutations MUST enforce CSRF. The web UI MUST display the current usage alongside the policy and surface a remediation hint when the underlying filesystem does not support quotas.
