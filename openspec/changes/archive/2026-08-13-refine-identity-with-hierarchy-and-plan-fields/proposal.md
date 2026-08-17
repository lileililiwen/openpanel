# Refine identity with hierarchy-aware and plan-aware fields

## Why

`openspec/specs/identity/spec.md` defines a `User` aggregate with
`role` of `Owner`, `Admin`, `User`. It does not yet support a
**parent account** reference (for reseller hierarchy) or a
**hosting-plan** reference (for plan-driven quotas). Both are
required by the follow-on `add-hosting-plans` and
`add-account-hierarchy` changes. Without these fields, those
changes have no anchor in the user aggregate. This refinement adds
the fields and the placeholder logic — no behavioural change yet.

## What Changes

- New `User.parent_account_id: Option<UserId>` and
  `User.hosting_plan_id: Option<HostingPlanId>` fields.
- New `IdentityError::ParentAccountCycle` placeholder
  (detection is implemented in `add-account-hierarchy`).
- New repository helpers `find_children(parent_id)` and
  `find_by_plan(plan_id)` with placeholder implementations.

## Capabilities

### Modified Capabilities

- `identity`: parent_account_id and hosting_plan_id fields;
  placeholder cycle-detection error; placeholder lookup methods.

## Impact

- Domain: `User` aggregate gains two fields; `HostingPlanId` new
  newtype (full type lives in the new spec).
- App: SQLite migration adds two nullable columns + indexes.
- No endpoint change in this change.
