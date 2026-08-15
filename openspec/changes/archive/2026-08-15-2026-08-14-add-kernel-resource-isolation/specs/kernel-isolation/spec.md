## ADDED Requirements

### Requirement: Per-User cgroup v2 Limits

The system SHALL enforce per-user cgroup v2 limits for CPU, memory, IO,
and pids via a `CgroupEnforcer`. Each user SHALL have an isolated cgroup
slice under `/sys/fs/cgroup/openpanel/{user_id}`.

#### Scenario: Limits applied

- **WHEN** an Admin `PUT /admin/isolation/{u1}` with cpu, memory, and
        pids limits
- **THEN** the corresponding `cpu.max`, `memory.max`, and `pids.max` are
        written to user `u1`'s slice.

#### Scenario: Slice isolation

- **WHEN** user `u1`'s processes exceed `pids.max`
- **THEN** further fork/clone is denied for `u1` only and neighbours are
        unaffected.

### Requirement: User-Namespace and PID Isolation

The system SHALL support user-namespace isolation and a separate PID
namespace for a user's processes when enabled in the policy.

#### Scenario: Namespace isolation enabled

- **WHEN** an Admin enables `userns_enabled` and `pid_isolation` for
        user `u1`
- **THEN** user `u1`'s processes run in a new user namespace and a new
        PID namespace, isolated from the host and other users.

### Requirement: Quota Bridge

The system SHALL bridge the `resource-quotas` plan caps (CPU shares, RAM,
process cap) onto cgroup limits so a user's plan quota becomes kernel
enforcement.

#### Scenario: Quota becomes cgroup limit

- **WHEN** a plan quota with RAM and process caps is bridged for user
        `u1`
- **THEN** `memory.max` and `pids.max` are derived from the plan and
        applied by the `CgroupEnforcer`.

### Requirement: Enforcement Failure Safety

If a cgroup write fails, the system SHALL log and audit the failure and
SHALL NOT silently disable isolation for the user.

#### Scenario: Write failure logged

- **WHEN** a cgroup write for user `u1` fails
- **THEN** an audit `IsolationEnforceFailed{user}` is recorded and
        isolation for `u1` is retained (process spawn blocked until
        resolved), not silently skipped.

### Requirement: Isolation Policy Management

The system SHALL let an authorised caller read and update a per-user
isolation policy and a default policy applied to users without an
explicit override.

#### Scenario: Read policy

- **WHEN** an Admin `GET /admin/isolation/{u1}`
- **THEN** the current `IsolationPolicy` for `u1` is returned.

#### Scenario: Default policy applied

- **WHEN** a user has no explicit policy and a default policy exists
- **THEN** the default policy is applied for that user.
