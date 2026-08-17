## Purpose

Introduces a parent-child user relationship with quota pooling,
the basis for reseller-style multi-tenant SaaS deployments on
OpenPanel. A parent account owns zero or more children; some
quotas are "pooled" — shared across children up to a parent-
declared total. Cycles are forbidden; tree structures are
strictly arborescent.

# account-hierarchy Specification

## Requirements

### Requirement: Account Relationship

The account-hierarchy context SHALL model an `AccountRelationship` aggregate. A user MAY have at most one parent. Each relationship is `(parent_id, child_id, created_at, created_by, status)`. The repository MUST expose `parent_of(child_id) -> Option<parent>` and `children_of(parent_id) -> Vec<child>` lookups. Cycles MUST be detected at create time.

#### Scenario: Create child

- **WHEN** an Owner creates a child under parent `p1`
- **THEN** a new relationship `(p1, c1, Active)` exists and `parent_of(c1) == Some(p1)`.

#### Scenario: Cycle rejected

- **WHEN** a user with parent `p1` is asked to become a child of `p2` and `parent_of(p2) == c1` (a cycle `c1 → p2 → p1 → c1` is formed if allowed)
- **THEN** the create call is rejected with `AccountHierarchyError::Cycle{path}` listing `["c1","p2","p1","c1"]`.

#### Scenario: Detach

- **WHEN** an Owner detaches a child
- **THEN** the relationship becomes `Detached` and is preserved for audit but is no longer current.

### Requirement: Quota Pools

The system SHALL let Owners declare `QuotaPool{ parent_id, axis, total_bytes, created_at }` for axes including at minimum `BandwidthBytesPerMonth`, `DiskBytes`. A child MAY request a `quota_pool_share` for that axis when being created or updated; the sum of children's claims MUST NOT exceed the parent's `total_bytes`. The reconciler SHALL run every 60 seconds and update each child's `effective_share = min(plan_cap, parent_pool.total - sibling_total_claims)`.

#### Scenario: Declared pool

- **WHEN** an Owner declares a pool of 5TB Bandwidth on parent `p1`
- **THEN** the pool is recorded and exposed via `GET /users/{p1}/quota-pool`.

#### Scenario: Exceeding pool

- **WHEN** the parent's existing children already claim 4.5TB and a new child requests 1TB
- **THEN** the create/claim is rejected with `QuotaPoolError::Exhausted{axis="BandwidthBytesPerMonth"}`.

#### Scenario: Reconciler redistributes

- **WHEN** a child is detached
- **THEN** within 60s the remaining children's `effective_share` is recomputed and the pool total is unchanged.

### Requirement: Child Account Creation

The system SHALL let a parent or an Owner create a child user. The new user's role MUST be lower privilege than the creator's (Owner → {Admin, User}; Admin → User). The new user's plan defaults to a clone of the creator's plan. The new user's `parent_account_id` is set and cannot be edited except by an Owner or the parent.

#### Scenario: Admin creates a User child

- **WHEN** an Admin user `a1` posts to `POST /users/{a1}/children` with `role=User`
- **THEN** the child is created, `parent_account_id == a1.id`, audit `ChildAccountCreated{by=a1, child=c1}` is recorded.

#### Scenario: User cannot create a child

- **WHEN** a User-role principal posts to `POST /users/{u1}/children`
- **THEN** the request is rejected with 403.

#### Scenario: Higher-privilege role requested

- **WHEN** an Admin posts with `role=Owner`
- **THEN** the request is rejected with `AccountHierarchyError::RoleEscalationForbidden`.

### Requirement: Account Tree Rendering

The web UI SHALL render the recursive descendant tree at `/accounts/tree` with indentation, role badges, and a usage bar for each pooled axis. The CLI SHALL output the tree as a JSON object whose schema is documented.

#### Scenario: Tree JSON

- **WHEN** an Owner runs `openpanel user tree <root_id> --json`
- **THEN** the output is `{root: { id, role, children: [ {…, children: […]} ] } }` covering every descendant.

#### Scenario: Web tree depth limit

- **WHEN** a tree exceeds depth 8
- **THEN** the renderer collapses deeper nodes behind a
        "show more" disclosure and prints a depth badge.

### Requirement: Delete Behaviour

`DELETE /api/v1/users/{id}` SHALL refuse to delete a user with `children_of(id).any()`. The Owner MAY detach children individually before deleting the parent. Soft-delete retains the row for audit with `disabled_at` set.

#### Scenario: Delete refuses when children exist

- **WHEN** an Owner attempts to delete a user with children
- **THEN** the response is `409` with `children_present` and lists the child ids; no row is removed.
