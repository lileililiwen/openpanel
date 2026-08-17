## ADDED Requirements

### Requirement: Parent Account and Hosting Plan Fields

The User aggregate SHALL record two optional identity references: `parent_account_id: Option<UserId>` (used by the reseller hierarchy change) and `hosting_plan_id: Option<HostingPlanId>` (used by the hosting-plan change). Both SHALL be nullable. Self-parenting SHALL be rejected by `User::new` with `IdentityError::ParentAccountCycle`. The repository trait SHALL expose placeholder methods `find_children(parent_id)` and `find_by_plan(plan_id)` that return empty until the corresponding follow-on changes ship.

#### Scenario: Create a user with both fields

- **WHEN** an Owner creates a user with `parent_account_id=u1, hosting_plan_id=p1`
- **THEN** the user is persisted and both fields round-trip through SQLite.

#### Scenario: Self-parenting rejection

- **WHEN** an Owner creates a user with `parent_account_id=u_new.id`
- **THEN** creation fails with `IdentityError::ParentAccountCycle` and no row is written.

#### Scenario: Backfilled users have NULL on both fields

- **WHEN** the migration runs on an existing DB
- **THEN** every existing row has `parent_account_id IS NULL AND hosting_plan_id IS NULL`.

### Requirement: Repository Lookup Helpers (Placeholders)

The `UserRepository` trait SHALL expose `find_children(parent_id)` and `find_by_plan(plan_id)`. Their behaviour is deliberately empty until the follow-on changes ship; they MUST be safe to call.

#### Scenario: Helper called before follow-on changes land

- **WHEN** a caller invokes either placeholder
- **THEN** it returns an empty list and does not error.

#### Scenario: Backed by indexed columns

- **WHEN** the migration is complete and helpers are re-implemented by the follow-on changes
- **THEN** the indexes `idx_users_parent_account_id` and `idx_users_hosting_plan_id` are used by the queries.
