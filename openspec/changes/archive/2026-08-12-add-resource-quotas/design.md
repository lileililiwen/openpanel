# Add per-user resource quotas — Design

## Domain model

```
QuotaPolicy
  { id, subject_kind: User | Site, subject_id,
    disk_soft_mb, disk_hard_mb, disk_grace_days,
    bandwidth_soft_mb_per_month, bandwidth_hard_mb_per_month,
    inode_soft, inode_hard,
    max_file_size_mb?,
    cpu_shares? }

QuotaUsage
  { subject_id, sampled_at,
    disk_used_mb, disk_inodes_used,
    bandwidth_used_mb_this_month,
    over_soft: { disk, bandwidth, inodes } : Set<Dim>,
    over_hard: { ... } : Set<Dim> }
```

## Disk enforcement

- Linux: `nix::unistd::quotactl` with `QIF_BLIMITS` /
  `QIF_INODES`. Filesystem must be mounted with `usrquota`/`grpquota`
  (panel refuses to enable quotas on a non-quota-capable mount and
  records the reason in audit).
- Grace windows: configurable (default 7 days).
- Block-on-hard: the kernel returns `EDQUOT`; the file manager
  surfaces a typed `QuotaExceeded` error to the API.
- The panel never edits `/etc/fstab`; the operator mounts the
  filesystem with quota support at install time and the panel
  detects and uses it.

## Bandwidth enforcement

- Per-UID shaping via `tc` with a classful qdisc on the egress
  interface.
- The panel measures bytes-out per UID by reading
  `/proc/net/xt_qtaguid/stat` or, on modern kernels,
  `nftables` byte counters per mark.
- Monthly reset on the first day of the configured month boundary
  (defaults to the first second of UTC month).
- Soft threshold: 90% of bandwidth sends a warning event.
- Hard threshold: `tc` changes the class rate-limit to 0, blocking
  new egress. Inbound is not blocked (the panel never breaks SSH or
  the API).

## Inode enforcement

- The in-process counter walks the user's home tree once at quota
  set and at every periodic sample (default 5 min).
- Hard limit: `nlink` / file creation returns `EDQUOT` via
  `setquota` inode hard limit.
- The counter MUST be cancellable and resumable; long walks are
  checkpointed.

## CPU shares

- Optional: `systemd-run --user --scope -p CPUWeight=…` for
  per-user service scopes. The panel documents this as best-effort
  and never promises strict isolation.

## Sampling

The existing monitoring background task is extended with a quota
sampler. The sampler emits `quota.usage` rows and triggers
`AlertFired` events when soft/hard thresholds are crossed (using
the existing hysteresis model). The notification dispatcher
(separate OpenSpec change) consumes those events.

## Endpoints

```
GET    /api/v1/users/{id}/quotas
PUT    /api/v1/users/{id}/quotas
GET    /api/v1/users/{id}/quotas/usage
GET    /api/v1/sites/{id}/quotas
PUT    /api/v1/sites/{id}/quotas
GET    /api/v1/sites/{id}/quotas/usage
```

## Failure modes

- Quota-capable filesystem missing: panel returns 503 with a clear
  remediation step ("mount with `usrquota`").
- `tc` not available: bandwidth quota is reported but not enforced;
  audit logs the degraded state.
- Quota removed out-of-band: the next sample re-syncs the policy
  to the kernel.

## Tests

```
1.1  Unit: QuotaPolicy validation, soft/hard math, grace window
     evaluation, monthly bandwidth reset.
1.2  Property: usage over hard always blocks; usage under soft
     never blocks; grace window advances correctly across time.
1.3  Service tests with mock disk/bandwidth/inode enforcers and
     mock clock.
1.4  Integration: full disk -> EDQUOT at file manager; bandwidth
     over soft -> warning; over hard -> egress blocked.
1.5  CLI E2E: `openpanel quota set/get/usage` and
     `openpanel quota disable`.
1.6  Web: /users/{id}/quotas with policy form, usage chart,
     and CSRF.
```
