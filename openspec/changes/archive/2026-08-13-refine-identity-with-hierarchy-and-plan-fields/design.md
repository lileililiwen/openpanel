# Refine identity with hierarchy-aware and plan-aware fields — Design

## New fields

```rust
pub struct User {
    // ... existing fields ...
    pub parent_account_id: Option<UserId>,
    pub hosting_plan_id: Option<HostingPlanId>,
}

newtype_struct!(HostingPlanId, Uuid);
```

The fields are nullable. `parent_account_id != id` is the only
invariant enforced in this change (self-parenting disallowed).

## Migration

```sql
ALTER TABLE users ADD COLUMN parent_account_id TEXT;
ALTER TABLE users ADD COLUMN hosting_plan_id TEXT;
CREATE INDEX idx_users_parent_account_id ON users(parent_account_id);
CREATE INDEX idx_users_hosting_plan_id ON users(hosting_plan_id);
```

## Lookup helpers

```rust
trait UserRepository {
    fn find_children(&self, parent: UserId) -> Vec<User> { vec![] }
    fn find_by_plan(&self, plan: HostingPlanId) -> Vec<User> { vec![] }
}
```

Both default to empty until the follow-on changes. The placeholders
exist so the follow-on changes need not edit trait signatures.

## Tests

```
1.1  Unit: User::new rejects self-parenting; SQL round-trip
      preserves both fields.
1.2  Property: parent_account_id != id; both fields are nullable;
      backfill rows have NULL.
1.3  Service tests: migration backfills correctly; placeholders
      return empty before follow-on changes.
```
