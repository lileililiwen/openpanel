# Refine identity with hierarchy-aware and plan-aware fields — Tasks

## 1. Testing

- [x] 1.1 Unit tests for self-parenting rejection and SQL
      round-trip of the new fields.
- [x] 1.2 Property tests: parent_account_id != id; both fields
      nullable; backfill is null.
- [x] 1.3 Service tests: migration backfills; placeholder
      find_children / find_by_plan return empty.

## 2. Domain and Application

- [x] 2.1 Extend `User` aggregate with the two fields.
- [x] 2.2 Add the SQLite migration.
- [x] 2.3 Add `HostingPlanId` newtype and placeholder repo
      helpers.

## 3. Adapters and UI

- [x] 3.1 Update OpenAPI / CLI JSON output to expose the
      optional fields.
- [x] 3.2 No web UI change in this change (parent/plan edit
      surfaces ship with the follow-on changes).

## 4. Validation

- [x] 4.1 `cargo test --workspace` twice.
- [x] 4.2 `make check` clean (modulo pre-existing clippy/doc
      nits that are unrelated to this change).
- [x] 4.3 Smoke-test: a user with `parent_account_id=self.id`
      rejection is exercised.
- [x] 4.4 Archive with `openspec archive refine-identity-with-hierarchy-and-plan-fields`.
