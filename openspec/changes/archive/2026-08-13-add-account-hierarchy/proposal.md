# Add account hierarchy

## Why

`openspec/specs/identity/spec.md` already supports the
`Owner / Admin / User` roles. It does not yet model a
**reseller hierarchy** where one account owns child accounts
that share part of a parent quota pool (a "reseller plan").
cPanel's WHM, Plesk's reseller tier, and Baota's commercial
edition all support this. OpenPanel RBAC currently treats every
account as a peer. This change adds a `parent_account_id`
relationship, child-account creation by parents, and quota
inheritance — forming the foundation of a multi-tenant SaaS
deployment.

## What Changes

- New bounded context `account-hierarchy` carrying the
  `AccountRelationship` aggregate and `HierarchyService`.
- New API: `POST /api/v1/users/{id}/children`,
  `GET /api/v1/users/{id}/tree`,
  `DELETE /api/v1/users/{id}` (refuses while children exist),
  `POST /api/v1/users/{id}/quota-pool` for pooled resources.
- New SQLite tables: `account_relationships(parent_id, child_id,
  created_at)`, `quota_pools(parent_id, axis, total_bytes)`.
- Quota inheritance: a parent's pooled quota is the **sum**
  cap shared across children of that axis; the parent's own use
  is excluded from the pool until the child unassigns.

## Capabilities

### New Capabilities

- `account-hierarchy`: parent-child user relationships and quota
  pooling.

## Impact

- Domain: `AccountRelationship`, `QuotaPool`, `HierarchyNode`.
- App: `HierarchyService`, child-account creation flow
  (defaults: same role as creator minus one, with the parent's
  plan cloned), `PoolEnforcer` consulted by `PlanResolver`.
- API/CLI/web: `/api/v1/users/{id}/children`,
  `/api/v1/users/{id}/tree`; CLI
  `openpanel user {create-child,delete,assign-pool}`; web
  `/accounts/tree` page.
- Coupling: depends on `refine-identity-with-hierarchy-and-plan-fields`
  for `parent_account_id` and on `add-hosting-plans` for plan
  resolution.
