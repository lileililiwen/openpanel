# Add account hierarchy — Design

## Relationship model

```rust
pub struct AccountRelationship {
    pub parent_id: UserId,
    pub child_id: UserId,
    pub created_at: DateTime<Utc>,
    pub created_by: UserId,
    pub status: HierarchyStatus,   // Active | Detached
}
```

A user MAY have at most one parent. A cycle (A → B → A) is
detected at creation time and rejected with
`AccountHierarchyError::Cycle{path}` containing the user ids.

## Pooled quotas

A parent may declare quota pools for axes such as
`BandwidthBytesPerMonth`. When a pool exists, the effective per-
child quota is `min(role_default, plan_cap, parent_pool_share)`,
where `parent_pool_share = pool.total - sum(current_child_usage)`
for that axis on a snapshot. The snapshot is recomputed every 60s
by a background reconciler.

## Child-account creation

```
POST /api/v1/users/{parent_id}/children
  body: { username, email, password, role: "Admin" | "User",
          initial_plan_id?: PlanId,
          quota_pool_share?: { axis, share } }
```

Rules:
- Caller must be the parent or an Owner.
- `role` must be lower privilege than the caller
  (Owner → Admin/User; Admin → User; User cannot create children).
- Default plan is the parent's plan cloned.
- `quota_pool_share` may claim bytes from a parent pool; total
  claimed by children ≤ pool total.

## Endpoints

```
GET    /api/v1/users/{id}/tree                       recursive descendants
POST   /api/v1/users/{id}/children                   create a child
DELETE /api/v1/users/{id}                            refuses if children exist
POST   /api/v1/users/{id}/quota-pool                 declare / update pool
GET    /api/v1/users/{id}/quota-pool/usage           current usage breakdown
```

## CLI

```
openpanel user create-child <parent_id> \
  --username <u> --email <e> --password <p> --role Admin
openpanel user delete <user_id>           # refuses when children exist
openpanel user tree <root_id>
openpanel pool set <parent_id> --bandwidth <bytes>
openpanel pool usage <parent_id>
```

## Tests

```
1.1  Unit: cycle detection; pool math; tree flatten.
1.2  Property: every user has ≤ 1 parent; tree never contains
      a cycle; pool total ≥ sum(child_share).
1.3  Service tests with mock resolver and audit: create-child,
      delete, pool set, pool usage snapshot.
1.4  Integration: full tree + pool lifecycle through REST.
1.5  CLI E2E: tree, create-child, delete-with-children-fails.
1.6  Web: /accounts/tree renders indented tree with usage bars.
```
