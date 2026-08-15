# Add Kernel resource isolation

## Why

The 2026 baseline (Panelica's 5-layer isolation, CloudLinux) expects
per-user cgroup v2 plus namespace isolation for safe multi-tenant
hosting. OpenPanel's active `resource-quotas` spec tracks per-plan
quotas but enforces nothing at the kernel level; cgroup currently
appears only as a CPU-calculation nuance in archived `monitoring`. For
untrusted multi-tenant hosting this is a hard requirement: without kernel
isolation, one user's runaway process can starve or escape into
neighbours. This change adds a new `kernel-isolation` bounded context
that enforces real limits and bridges to the `resource-quotas` quota
axis so plan caps become cgroup limits.

## What Changes

- A new `kernel-isolation` capability providing a `CgroupEnforcer`.
- Per-user cgroup v2 limits for CPU, memory, IO, and pids.
- User-namespace and PID isolation for user processes.
- A bridge that maps `resource-quotas` plan caps onto cgroup limits.
- New endpoints: `GET /admin/isolation/{user_id}`,
  `PUT /admin/isolation/{user_id}`, `GET /admin/isolation/policy`,
  `PUT /admin/isolation/policy`.
- Coupling: depends on `resource-quotas` (quota axis), `account-hierarchy`
  (user identity), `hosting-plans` (plan caps).

## Capabilities

### New Capabilities

- `kernel-isolation`: enforce per-user cgroup v2 limits (CPU, memory,
  IO, pids) and user-namespace/PID isolation, bridged from the
  `resource-quotas` quota axis.

## Impact

- Domain: `IsolationPolicy`, `CgroupLimit`, `UserNamespaceConfig`.
- App: `CgroupEnforcer`, `NamespaceIsolator`, `QuotaBridge`.
- API/CLI/web: `/admin/isolation/{user_id}`, `/admin/isolation/policy`;
  CLI `openpanel admin isolation {get,set}`; admin Isolation tab.
- Security: cgroup writes scoped to the user's slice; failed enforcement
  is logged but never silently disables isolation.
- Coupling: depends on `resource-quotas` for quota values,
  `account-hierarchy` for user resolution, `hosting-plans` for plan caps.
