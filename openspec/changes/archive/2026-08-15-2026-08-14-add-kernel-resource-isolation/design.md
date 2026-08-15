# Add Kernel resource isolation — Design

## IsolationPolicy model

```rust
pub struct IsolationPolicy {
    pub user_id: UserId,
    pub cpu_max: CpuLimit,        // cgroup v2 cpu.max
    pub memory_max: Bytes,        // cgroup v2 memory.max
    pub io_max: Option<IoLimit>,  // cgroup v2 io.max
    pub pids_max: u32,            // cgroup v2 pids.max
    pub userns_enabled: bool,     // user-namespace isolation
    pub pid_isolation: bool,      // separate PID namespace
}
```

## CgroupEnforcer flow

```
apply(user_id, policy):
  slice = format!("/sys/fs/cgroup/openpanel/{user_id}")
  mkdir slice (0700) if absent
  write cpu.max, memory.max, io.max, pids.max
  if userns_enabled: spawn user processes in new userns
  if pid_isolation: enter new pid namespace
  on write failure -> log + audit IsolationEnforceFailed{user}
    (never silently skip)
```

## QuotaBridge flow

```
bridge(user_id, plan_quota):
  cpu_max   = plan_quota.cpu_shares -> cgroup cpu.max
  memory_max = plan_quota.ram_mb  -> cgroup memory.max
  pids_max  = plan_quota.process_cap -> cgroup pids.max
  apply(user_id, derived IsolationPolicy)
```

## Endpoints

```
GET  /api/v1/admin/isolation/{user_id}
PUT  /api/v1/admin/isolation/{user_id}  body { cpu_max, memory_max, io_max?, pids_max, userns_enabled, pid_isolation }
GET  /api/v1/admin/isolation/policy
PUT  /api/v1/admin/isolation/policy     body { defaults per plan }
```

## Tests

```
1.1 Unit: cgroup limit mapping from quota; namespace flag application;
    enforce-failure path does not disable isolation.
1.2 Property: cgroup write path stays under /sys/fs/cgroup/openpanel;
    derived limits never negative.
1.3 Service tests w/ mock cgroup fs: apply, bridge from quota, update,
    failure logging.
1.4 Integration: live cgroup slice created with correct cpu/mem/pids;
    unauthorised user cannot cross slice.
1.5 CLI E2E: `openpanel admin isolation get` -> `set`.
1.6 Web: admin Isolation tab (CSRF), per-user limits form, policy form.
```
