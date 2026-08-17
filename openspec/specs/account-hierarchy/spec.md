# account-hierarchy Specification

## Purpose

TBD - created by archiving change add-account-hierarchy. Update Purpose after archive.

## Requirements

### Requirement: Hierarchy Tree and Cycle Detection

The account-hierarchy context SHALL model a parent-child relationship that backs the `User.parent_account_id` field added by the `refine-identity-with-hierarchy-and-plan-fields` change. A user MAY have at most one active parent. The `would_cycle` helper SHALL detect any direct or transitive cycle by walking DOWN from the proposed parent to see if the child is reachable; the persistence layer shall refuse to insert a relationship that would create a cycle.

#### Scenario: Self-parenting is rejected

- **WHEN** an Owner submits a parent assignment with `parent_id == child_id`
- **THEN** the assignment is rejected with `AccountHierarchyError::Cycle`.

#### Scenario: Transitive cycle is rejected

- **WHEN** a parent chain `c -> b -> a` is in place and an Owner proposes `c -> a` to form `c -> a -> b -> c`
- **THEN** the assignment is rejected with `AccountHierarchyError::Cycle`.

#### Scenario: Tree contains no cycles

- **WHEN** the panel is queried for `/users/{id}/tree`
- **THEN** every node in the returned tree has at most one parent and the tree contains no cycles.

### Requirement: Pooled Quota Caps

A parent MAY declare a `QuotaPool` for an axis (`DiskBytes`, `BandwidthBytesPerMonth`, `MaxChildAccounts`). A child MAY claim bytes from a parent's pool via `PoolClaim`. The sum of `share_bytes` for a `(parent, axis)` MUST NOT exceed `pool.total_bytes`; the `check_pool_claim` helper returns `PoolExhausted` when the requested share would overflow the pool.

#### Scenario: Claim within pool succeeds

- **WHEN** a parent's bandwidth pool is 1 TiB and 3 children each claim 300 GiB
- **THEN** all three claims are persisted and the pool usage is 900 GiB.

#### Scenario: Claim exhausts pool

- **WHEN** a parent's pool already has 900 GiB claimed and a fourth claim requests any positive bytes that would overflow the remaining capacity
- **THEN** the claim is rejected with `AccountHierarchyError::PoolExhausted`.

### Requirement: Child-Account Creation

A parent MAY create a child account via `POST /users/{parent_id}/children`. The role matrix SHALL allow Owner to create Admin or User, Admin to create User, and refuse every other combination. The created child SHALL be persisted with `parent_account_id = parent_id` and the new parent record SHALL be visible in `/users/{parent_id}/tree`.

#### Scenario: Owner creates a User child

- **WHEN** an Owner submits `POST /users/{me_id}/children` with `role: "user"`
- **THEN** the user is created with `parent_account_id = me_id` and appears in the tree.

#### Scenario: Admin cannot create another Admin

- **WHEN** an Admin submits `POST /users/{me_id}/children` with `role: "admin"`
- **THEN** the request is rejected with `AccountHierarchyError::Forbidden`.

### Requirement: Audit and Event Surface

Every hierarchy lifecycle mutation SHALL emit an audit event with the actor, the parent id, the child id, and one of `{AccountHierarchyChildCreated, AccountHierarchyAttached, AccountHierarchyDetached, AccountHierarchyPoolSet, AccountHierarchyPoolClaimed, AccountHierarchyPoolReleased}`.

#### Scenario: Pool claim is audited

- **WHEN** a child claims 100 GiB from a parent's pool
- **THEN** the audit log records `AccountHierarchyPoolClaimed{actor, parent_id, child_id, axis, share_bytes}`.
